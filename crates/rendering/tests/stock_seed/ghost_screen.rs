//! Original D3D9 blur/composite pixels over the same Vulkan world raster.

#![allow(unsafe_code)]

use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    M2LocalLightState, M2SceneUniform, TerrainSceneUniform, UiMeshPlan, UiRenderBlend,
    UiRenderQuad, UiRenderSource, UiShaderSource, UiTextureAddressMode, UiTextureResidency,
    VulkanBootstrap, WorldCamera, WorldFrameScene, WorldFrameScreenEffect, WorldModelSceneUniform,
    WorldScreenWindow, WorldSkyDome, WorldSkyFrame,
};
use std::error::Error;

fn scene() -> WorldFrameScene<'static> {
    WorldFrameScene::new(
        TerrainSceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
        ),
        WorldModelSceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
        ),
        M2SceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    )
}

#[test]
fn ghost_screen_matches_native_blur_and_composite() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let export = std::env::var_os("SOLARITY_SCREEN_EFFECT_INPUTS").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path)?;
    }
    let fixture = include_bytes!("../fixtures/ghost_screen_native.bin");
    assert_eq!(&fixture[..4], b"GHST");
    let mut offset = 8;
    let word = |at| {
        u32::from_le_bytes([
            fixture[at],
            fixture[at + 1],
            fixture[at + 2],
            fixture[at + 3],
        ])
    };
    let mut cases = 0;
    for (width, height) in [(64, 64), (65, 61), (80, 48)] {
        let window = video
            .window("Solarity native ghost screen", width, height)
            .vulkan()
            .hidden()
            .build()?;
        let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
        // SAFETY: SDL transfers sole surface ownership and the window outlives the renderer.
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
        let frame = scene().with_sky(WorldSkyFrame::new(&sky, camera));
        renderer.request_frame_capture()?;
        renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let baseline = renderer.take_captured_frame()?.ok_or("base capture")?;
        if let Some(path) = &export {
            std::fs::write(
                path.join(format!("world-{width}-{height}.rgba")),
                baseline.rgba8(),
            )?;
            continue;
        }
        for glow in [0_f32, 0.2, 0.5, 1., 1.25, 0.5 / 255., 1.5 / 255.] {
            assert_eq!((word(offset), word(offset + 4)), (width, height));
            assert_eq!(word(offset + 8), glow.to_bits());
            let native_color = word(offset + 12);
            assert_eq!(
                WorldFrameScreenEffect::ghost(glow),
                WorldFrameScreenEffect::Ghost {
                    glow: (native_color >> 24) as u8,
                }
            );
            let size = (width * height * 4) as usize;
            assert_eq!(baseline.rgba8(), &fixture[offset + 16..offset + 16 + size]);
            let expected = &fixture[offset + 16 + size..offset + 16 + size * 2];
            renderer.request_frame_capture()?;
            renderer.present_world_frame(
                frame.with_screen_effect(Some(WorldFrameScreenEffect::ghost(glow))),
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
            let actual = renderer.take_captured_frame()?.ok_or("ghost capture")?;
            let maximum = actual
                .rgba8()
                .iter()
                .zip(expected)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
            assert!(
                maximum <= 2,
                "{width}x{height} glow={glow} maximum channel error={maximum}"
            );
            offset += 16 + size * 2;
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
            frame.with_screen_effect(Some(WorldFrameScreenEffect::ghost(1.))),
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
        renderer.request_frame_capture()?;
        renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let restored = renderer.take_captured_frame()?.ok_or("restored capture")?;
        assert_eq!(restored.rgba8(), baseline.rgba8());
    }
    if export.is_none() {
        assert_eq!(cases, 21);
        assert_eq!(word(4), cases);
        assert_eq!(offset, fixture.len());
    }
    Ok(())
}
