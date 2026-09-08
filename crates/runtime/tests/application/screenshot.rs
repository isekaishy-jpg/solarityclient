//! Actual GPU readback and image codecs, including failed saves and repeated requests.

#![allow(unsafe_code)]

use std::error::Error;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use solarity_cpu::CpuPoolConfig;
use solarity_rendering::VulkanBootstrap;

use super::*;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK};

#[test]
fn screenshot_quality_matches_native_conversion() -> Result<(), Box<dyn Error>> {
    for row in include_str!("../fixtures/screenshot_quality_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = row.split_whitespace();
        let encoded = words.next().ok_or("quality input")?;
        let bytes = (0..encoded.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16))
            .collect::<Result<Vec<_>, _>>()?;
        let value = std::str::from_utf8(&bytes)?;
        let expected = words.next().ok_or("quality result")?.parse::<u8>()?;
        assert_eq!(
            ScreenshotRequest::new(false, "jpeg", value).quality,
            expected
        );
    }
    Ok(())
}

#[test]
fn screenshot_saves_the_completed_gpu_frame_and_reports_io_failure() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity screenshot test", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap has the window's required extensions and owns the surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives this sole renderer owner.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let extent = renderer.report().extent();
    let pixels = (0..extent.1)
        .flat_map(|y| {
            (0..extent.0).flat_map(move |x| {
                if y < extent.1 / 2 {
                    [230, 40, 20, 255]
                } else if x < extent.0 / 2 {
                    [20, 210, 40, 255]
                } else {
                    [20, 40, 220, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    let mut screenshots = RuntimeScreenshots::new(fixture.profile_root());
    for format in ["TGA", "jpeg"] {
        let request = ScreenshotRequest::new(true, format, "3");
        screenshots.request(request);
        screenshots.request(request); // native pre-present requests coalesce
        assert!(screenshots.poll(&mut renderer, &cpu).is_none());
        assert!(screenshots.poll(&mut renderer, &cpu).is_none()); // not presented yet
        renderer.present_rgba8(extent, &pixels)?;
        renderer.present_clear([extent.0 as f32, extent.1 as f32])?;
        let completion = wait_for_save(&mut screenshots, &mut renderer, &cpu)?;
        assert!(completion.world);
        let path = completion.result?;
        let decoded = image::open(&path)?.to_rgba8();
        assert_eq!(decoded.dimensions(), extent);
        if request.tga {
            assert_eq!(
                decoded.as_raw(),
                &pixels,
                "TGA keeps exact channels and row order"
            );
        } else {
            for (x, y) in [(8, 8), (8, 48), (48, 48)] {
                let offset = ((y * extent.0 + x) * 4) as usize;
                for (actual, expected) in decoded
                    .get_pixel(x, y)
                    .0
                    .iter()
                    .zip(&pixels[offset..offset + 4])
                {
                    assert!(
                        actual.abs_diff(*expected) <= 5,
                        "JPEG preserves the first captured frame"
                    );
                }
            }
        }
        assert!(screenshots.poll(&mut renderer, &cpu).is_none());
        assert!(screenshots.pending.is_none() && screenshots.capturing.is_none());
    }
    assert_eq!(
        fs::read_dir(fixture.profile_root().join("Screenshots"))?.count(),
        2
    );

    // Existing files are preserved even when captures use the same local second.
    renderer.request_frame_capture()?;
    renderer.present_rgba8(extent, &pixels)?;
    let frame = renderer
        .take_captured_frame()?
        .ok_or("missing dated capture")?;
    let request = ScreenshotRequest::new(false, "tga", "10");
    let directory = fixture.profile_root().join("Screenshots");
    let first = write_dated_screenshot(&directory, &frame, request, "090826_142233")
        .map_err(|e| e.to_string())?;
    let second = write_dated_screenshot(&directory, &frame, request, "090826_142233")
        .map_err(|e| e.to_string())?;
    assert_eq!(
        first.file_name().and_then(|n| n.to_str()),
        Some("WoWScrnShot_090826_142233.tga")
    );
    assert_eq!(
        second.file_name().and_then(|n| n.to_str()),
        Some("WoWScrnShot_090826_142233_1.tga")
    );
    assert_eq!(fs::read(first)?, fs::read(second)?);

    let blocked = fixture.profile_root().join("blocked");
    fs::write(&blocked, b"existing user file")?;
    screenshots.directory = blocked.clone();
    screenshots.request(request);
    assert!(screenshots.poll(&mut renderer, &cpu).is_none());
    renderer.present_rgba8(extent, &pixels)?;
    let completion = wait_for_save(&mut screenshots, &mut renderer, &cpu)?;
    assert!(!completion.world);
    assert!(completion.result.is_err());
    assert_eq!(fs::read(&blocked)?, b"existing user file");
    cpu.shutdown()?;
    Ok(())
}

fn wait_for_save(
    screenshots: &mut RuntimeScreenshots,
    renderer: &mut VulkanRenderer,
    cpu: &CpuExecutor,
) -> Result<ScreenshotCompletion, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = screenshots.poll(renderer, cpu) {
            return Ok(completion);
        }
        if Instant::now() >= deadline {
            return Err("screenshot did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
