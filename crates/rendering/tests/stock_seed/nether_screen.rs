//! Complete invisibility composition against native producers and archive shaders.
#![allow(unsafe_code)]

use glam::Vec3;
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiShaderSource, UiTextureAddressMode,
    UiTextureResidency, WorldScreenWindow,
};
use solarity_rendering::{
    VulkanBootstrap, WorldCamera, WorldFrameScreenEffect, WorldNetherState, WorldSkyDome,
    WorldSkyFrame,
};
use std::error::Error;

#[test]
fn invisibility_matches_original_vertex_and_pixel_shaders() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let fixture = include_bytes!("../fixtures/nether_screen_gpu_native.bin");
    assert_eq!(&fixture[..4], b"NTHR");
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
            .window("Solarity native invisibility", width, height)
            .vulkan()
            .hidden()
            .build()?;
        let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
        // SAFETY: SDL transfers the surface to a renderer that remains inside its lifetime.
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
        let mut state = WorldNetherState::default();
        for _ in 0..10 {
            assert_eq!((word(offset), word(offset + 4)), (width, height));
            let delta = f32::from_bits(word(offset + 8));
            let axis =
                [word(offset + 12), word(offset + 16), word(offset + 20)].map(f32::from_bits);
            if word(offset + 24) != 0 {
                state.reset_fade();
            }
            let effect = state.advance(delta, axis);
            let size = width as usize * height as usize * 4;
            assert_eq!(baseline.rgba8(), &fixture[offset + 28..offset + 28 + size]);
            let expected = &fixture[offset + 28 + size..offset + 28 + size * 2];
            renderer.request_frame_capture()?;
            renderer.present_world_frame(
                scene.with_screen_effect(Some(WorldFrameScreenEffect::Nether(effect))),
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
            let capture = renderer.take_captured_frame()?.ok_or("nether capture")?;
            let maximum = capture
                .rgba8()
                .iter()
                .zip(expected)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
            assert!(
                maximum <= 2,
                "{width}x{height} frame {cases} delta={delta} max error={maximum}"
            );
            offset += 28 + size * 2;
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
        renderer.request_frame_capture()?;
        renderer.present_world_frame_with_ui(
            scene.with_screen_effect(Some(WorldFrameScreenEffect::Nether(
                state.advance(0.75, [1., 0., 0.]),
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
        let overlay = renderer.take_captured_frame()?.ok_or("nether UI capture")?;
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
    assert_eq!(cases, word(4));
    assert_eq!(offset, fixture.len());
    Ok(())
}
