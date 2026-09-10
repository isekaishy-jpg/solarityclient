//! Retained screen-filter sequences against original producers and D3D shaders.
#![allow(unsafe_code)]

use glam::Vec3;
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiShaderSource, UiTextureAddressMode,
    UiTextureResidency, VulkanBootstrap, WorldCamera, WorldFrameScreenEffect, WorldScreenWindow,
    WorldSkyDome, WorldSkyFrame, WorldSpecialState,
};
use std::error::Error;

#[test]
fn special_screen_matches_native_history_and_polar_composition() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let fixture = include_bytes!("../fixtures/special_gpu_native.bin");
    assert_eq!(&fixture[..4], b"SPGP");
    let word = |at| {
        u32::from_le_bytes([
            fixture[at],
            fixture[at + 1],
            fixture[at + 2],
            fixture[at + 3],
        ])
    };
    let captures = [
        0, 1, 2, 30, 90, 134, 135, 136, 200, 255, 256, 257, 259, 260, 269, 270,
    ];
    let mut offset = 8;
    let mut cases = 0;
    let mut maximum_error = 0;
    for (width, height) in [(64, 64), (65, 61), (80, 48)] {
        let window = video
            .window("Solarity native screen filter", width, height)
            .vulkan()
            .hidden()
            .build()?;
        let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
        // SAFETY: The renderer's owned surface remains inside SDL's window lifetime.
        let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
        let mut renderer = unsafe { bootstrap.attach_surface(surface, (width, height), 0) }?;
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
        let scene = super::ghost_screen::scene().with_sky(WorldSkyFrame::new(&sky, camera));
        renderer.request_frame_capture()?;
        renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let baseline = renderer.take_captured_frame()?.ok_or("base capture")?;
        let size = width as usize * height as usize * 4;
        let mut state = WorldSpecialState::default();
        for frame in 0..271 {
            if frame == 270 {
                // Resource loss clears images, but preserves the CPU ramp/cursor.
                drop(renderer);
                let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
                // SAFETY: The prior renderer released its surface; this one has the same window lifetime.
                let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
                renderer = unsafe { bootstrap.attach_surface(surface, (width, height), 0) }?;
            }
            if let Some(parameters) = match frame {
                0 => Some([0xffffffff, 6, 40, 0]),
                135 => Some([0x85130f1c, 1, 60, 0]),
                260 => Some([0xff000000, 1, 100, 0]),
                _ => None,
            } {
                state.select(parameters);
            }
            let effect = state.advance(1. / 60.);
            let capture = captures.contains(&frame);
            if capture {
                renderer.request_frame_capture()?;
            }
            renderer.present_world_frame(
                scene.with_screen_effect(Some(WorldFrameScreenEffect::Special(effect))),
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
            if capture {
                assert_eq!(
                    [word(offset), word(offset + 4), word(offset + 8)],
                    [width, height, frame]
                );
                assert_eq!(baseline.rgba8(), &fixture[offset + 60..offset + 60 + size]);
                let expected =
                    &fixture[offset + 60 + size + 131072..offset + 60 + size * 2 + 131072];
                let actual = renderer.take_captured_frame()?.ok_or("special capture")?;
                let maximum = actual
                    .rgba8()
                    .iter()
                    .zip(expected)
                    .map(|(&a, &b)| a.abs_diff(b))
                    .max()
                    .unwrap_or(0);
                maximum_error = maximum_error.max(maximum);
                assert!(
                    maximum <= 2,
                    "{width}x{height} frame {frame}: max pixel error {maximum}"
                );
                offset += 60 + size * 2 + 131072;
                cases += 1;
            }
            if frame == 90 {
                // Other owners and disabled draws leave Special's retained images alone.
                for effect in [Some(WorldFrameScreenEffect::ghost(0.5)), None, None] {
                    renderer.present_world_frame(
                        scene.with_screen_effect(effect),
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
                }
            }
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
        renderer.request_frame_capture()?;
        renderer.present_world_frame_with_ui(
            scene.with_screen_effect(Some(WorldFrameScreenEffect::Special(state.advance(1.)))),
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
        let overlay = renderer
            .take_captured_frame()?
            .ok_or("special UI capture")?;
        let middle = ((height / 2 * width + width / 2) * 4) as usize;
        assert_eq!(&overlay.rgba8()[middle..middle + 4], &[255, 0, 0, 255]);
        renderer.request_frame_capture()?;
        renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(
            renderer
                .take_captured_frame()?
                .ok_or("restored capture")?
                .rgba8(),
            baseline.rgba8()
        );
    }
    assert_eq!(offset, fixture.len());
    assert_eq!(cases, word(4));
    println!("{cases} native screen-filter captures; maximum channel difference {maximum_error}");
    Ok(())
}
