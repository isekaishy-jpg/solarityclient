//! Real GPU recording reuse, scaling, screenshot arbitration, and resize checks.

#![allow(unsafe_code)]

use std::error::Error;
use std::time::{Duration, Instant};

use solarity_rendering::{VulkanBootstrap, VulkanRenderer};

/// Waits at the test boundary; production polling never waits for capture fences.
fn collect(renderer: &mut VulkanRenderer, pixels: &mut [u8]) -> Result<Duration, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(timestamp) = renderer.poll_video_frame(pixels)? {
            return Ok(timestamp);
        }
        if Instant::now() >= deadline {
            return Err("GPU recording capture timed out".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Actual readback proves persistent BGRA pixels, fixed scaled output, and screenshot priority.
#[test]
fn video_capture_reuses_scaled_storage_and_survives_screenshots_and_resize()
-> Result<(), Box<dyn Error>> {
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut builder = video.window("Solarity video capture test", 1600, 900);
    builder.vulkan().hidden().resizable();
    let mut window = builder.build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled the live window's required surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The renderer receives sole surface ownership and is dropped before the window.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (1600, 900), 0) }?;
    assert_eq!(renderer.begin_video_capture()?, (1280, 720));
    let mut pixels = vec![0; 1280 * 720 * 4];
    for index in 0..3 {
        let timestamp = Duration::from_millis(index * 34);
        assert!(renderer.request_video_frame(timestamp)?);
        assert!(!renderer.request_video_frame(timestamp)?);
        let extent = renderer.report().extent();
        let rgba = [240, 35, 17, 255].repeat((extent.0 * extent.1) as usize);
        renderer.present_rgba8(extent, &rgba)?;
        assert_eq!(collect(&mut renderer, &mut pixels)?, timestamp);
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [17, 35, 240, 255])
        );
        assert!(renderer.poll_video_frame(&mut pixels)?.is_none());
    }
    renderer.request_frame_capture()?;
    assert!(!renderer.request_video_frame(Duration::from_millis(200))?);
    renderer.present_clear([1600.0, 900.0])?;
    assert!(renderer.take_captured_frame()?.is_some());
    window.set_size(800, 800)?;
    // SDL updates the native surface before presentation performs its normal retry.
    let mut events = sdl.event_pump()?;
    events.pump_events();
    renderer.present_clear([800.0, 800.0])?;
    assert!(renderer.request_video_frame(Duration::from_millis(300))?);
    let extent = renderer.report().extent();
    renderer.present_rgba8(
        extent,
        &[255, 0, 0, 255].repeat((extent.0 * extent.1) as usize),
    )?;
    collect(&mut renderer, &mut pixels)?;
    assert_eq!(&pixels[(360 * 1280 + 640) * 4..][..4], &[0, 0, 255, 255]);
    if renderer.report().extent() == (800, 800) {
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
    }
    assert!(renderer.end_video_capture()?);
    assert!(renderer.poll_video_frame(&mut pixels)?.is_none());
    assert_eq!(renderer.begin_video_capture()?, (720, 720));
    assert!(renderer.end_video_capture()?);
    Ok(())
}
