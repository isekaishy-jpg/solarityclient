//! Native normal-world producer and original D3D9 glow/wave pixels.

#![allow(unsafe_code)]

use glam::Vec3;
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiShaderSource, UiTextureAddressMode,
    UiTextureResidency, VulkanBootstrap, WorldCamera, WorldFrameScreenEffect, WorldScreenWindow,
    WorldSkyDome, WorldSkyFrame,
};
use std::error::Error;

// The separate systems oracle covers 4F7290 itself. Supply its unspilled return
// here so the renderer's 4F8770 byte store can be compared with the native caller.
fn inebriation(actual: u32, fake: u32) -> f64 {
    f64::from((actual as i32).max(fake as i32).min(100)) * f64::from(0.01_f32)
}

#[test]
fn normal_glow_inputs_match_native_player_and_water_producer() -> Result<(), Box<dyn Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/world_glow_producer_native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let words = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        if words[5] == 0 {
            // Native selection gating is covered by the runtime environment test.
            assert_eq!(&words[6..], &[0xdead_beef, 0x1234_5678]);
            continue;
        }
        let player = (words[4] != 0).then(|| inebriation(words[1], words[2]));
        assert_eq!(
            WorldFrameScreenEffect::normal(f32::from_bits(words[0]), player, words[3] != 0, 1234),
            WorldFrameScreenEffect::Normal {
                glow: (words[7] >> 24) as u8,
                blur: words[7] as u8,
                wave_time_ms: (words[6] != 0).then_some(1234),
            },
            "{line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 270);
    Ok(())
}

#[test]
fn world_glow_matches_native_blur_wave_and_composite() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let fixture = include_bytes!("../fixtures/world_glow_screen_native.bin");
    assert_eq!(&fixture[..4], b"WGLW");
    let word = |at| {
        u32::from_le_bytes([
            fixture[at],
            fixture[at + 1],
            fixture[at + 2],
            fixture[at + 3],
        ])
    };
    let mut offset = 8;
    let mut cases = 0;
    for (width, height) in [(64, 64), (65, 61), (80, 48)] {
        let window = video
            .window("Solarity native world glow", width, height)
            .vulkan()
            .hidden()
            .build()?;
        let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
        // SAFETY: SDL transfers sole surface ownership; the window outlives the renderer.
        let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
        let mut renderer = unsafe { bootstrap.attach_surface(surface, (width, height), 0) }?;
        assert_eq!(renderer.report().extent(), (width, height));
        let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::new(0., 0.7, 0.7), 1000.)
            .frame(width as f32 / height as f32)?;
        let mut sky = WorldSkyDome::new();
        sky.update_colors(
            [
                Vec3::new(0.05, 0.15, 0.35),
                Vec3::new(0.2, 0.05, 0.4),
                Vec3::new(0.7, 0.3, 0.1),
                Vec3::new(0.1, 0.5, 0.2),
                Vec3::new(0.9, 0.7, 0.3),
            ],
            Vec3::new(0.02, 0.04, 0.08),
            0.,
            0.,
            camera,
        );
        let frame = super::ghost_screen::scene().with_sky(WorldSkyFrame::new(&sky, camera));
        renderer.request_frame_capture()?;
        renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let baseline = renderer.take_captured_frame()?.ok_or("base capture")?;
        for _ in 0..25 {
            assert_eq!((word(offset), word(offset + 4)), (width, height));
            let glow = f32::from_bits(word(offset + 8));
            let actual = word(offset + 12);
            let fake = word(offset + 16);
            let wet = word(offset + 20) != 0;
            let time = word(offset + 24);
            let color = word(offset + 28);
            let effect =
                WorldFrameScreenEffect::normal(glow, Some(inebriation(actual, fake)), wet, time);
            assert_eq!(
                effect,
                WorldFrameScreenEffect::Normal {
                    glow: (color >> 24) as u8,
                    blur: color as u8,
                    wave_time_ms: wet.then_some(time),
                }
            );
            let size = (width * height * 4) as usize;
            assert_eq!(baseline.rgba8(), &fixture[offset + 32..offset + 32 + size]);
            let expected = &fixture[offset + 32 + size..offset + 32 + size * 2];
            renderer.request_frame_capture()?;
            renderer.present_world_frame(
                frame.with_screen_effect(Some(effect)),
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            )?;
            let capture = renderer
                .take_captured_frame()?
                .ok_or("world glow capture")?;
            let maximum = capture
                .rgba8()
                .iter()
                .zip(expected)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
            assert!(
                maximum <= 2,
                "{width}x{height} glow={glow} actual={actual} fake={fake} wet={wet} time={time} max error={maximum}"
            );
            offset += 32 + size * 2;
            cases += 1;
        }
        let overlay = UiRenderQuad::new(
            0,
            UiRenderSource::VertexColor,
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            UiTextureResidency::Blocking,
            false,
            [8., 8., width as f32 - 8., height as f32 - 8.],
            [[0.; 2]; 4],
            [[1., 0., 0., 1.]; 4],
        );
        let logical_extent = [width as f32, height as f32];
        let plan = UiMeshPlan::prepare(logical_extent, [overlay].into_iter())?;
        let mesh = renderer.upload_ui_mesh(&plan)?;
        let pipeline =
            renderer.prepare_ui_pipeline(UiShaderSource::VertexColor, UiRenderBlend::Alpha)?;
        let draws = [renderer.prepare_ui_draw(mesh, pipeline, None, &plan, 0)?];
        for wet in [false, true] {
            renderer.request_frame_capture()?;
            renderer.present_world_frame_with_ui(
                frame.with_screen_effect(Some(WorldFrameScreenEffect::normal(
                    1.,
                    Some(1.),
                    wet,
                    1234,
                ))),
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                WorldScreenWindow::FULL,
                logical_extent,
                &draws,
            )?;
            let overlay = renderer.take_captured_frame()?.ok_or("UI capture")?;
            let middle = ((height / 2 * width + width / 2) * 4) as usize;
            assert_eq!(&overlay.rgba8()[middle..middle + 4], &[255, 0, 0, 255]);
        }
        renderer.request_frame_capture()?;
        renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let restored = renderer.take_captured_frame()?.ok_or("restored capture")?;
        assert_eq!(restored.rgba8(), baseline.rgba8());
    }
    assert_eq!(cases, 75);
    assert_eq!(word(4), cases);
    assert_eq!(offset, fixture.len());
    Ok(())
}
