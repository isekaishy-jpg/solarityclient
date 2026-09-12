//! Real LightSkybox models through the retained world compositor and frame banks.

#[path = "sky_glare.rs"]
mod glare;
#[path = "world_model_sky_resources.rs"]
mod world_model;

use super::*;
use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale};
use solarity_rendering::{
    TerrainSceneUniform, VulkanBootstrap, WorldCamera, WorldFrameScene, WorldModelSceneUniform,
};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
#[allow(unsafe_code)]
fn installed_skyboxes_render_retain_flags_and_ignore_camera_translation()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = solarity_asset::AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    let animations = Arc::new(solarity_asset::AnimationDataCatalog::load(&mut store)?);
    let mut definitions = lights.skyboxes().cloned().collect::<Vec<_>>();
    definitions.sort_by_key(LightSkybox::id);
    let mut unique = std::collections::HashSet::new();
    definitions.retain(|row| unique.insert(row.model_path().to_uppercase()));
    let mut sky = RuntimeSkyResources::load(AssetStoreHandle::new(store), &lights, animations)?;
    assert!(
        sky.sources.iter().all(Option::is_some),
        "all five stock celestial textures must decode, including sunGlare's 0x88 alpha flag"
    );
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity installed skyboxes", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: SDL transfers sole surface ownership; its window outlives the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let mut random = crate::random::CrtRand::new();
    let mut visible_models = 0;
    let mut fading_models = 0;
    let mut models_checked = 0;
    for row in definitions {
        if row.model_path().is_empty() {
            continue;
        }
        let time = 10000 + models_checked * 1234;
        let mut original = Vec::new();
        for (case, opacity) in [1., 1., 0.4].into_iter().enumerate() {
            let eye = if case == 1 {
                Vec3::new(12000., -55000., 700.)
            } else {
                Vec3::ZERO
            };
            let camera =
                WorldCamera::stock(eye, eye + Vec3::new(1., 0., 0.5), Vec3::Z, 1000.).frame(1.)?;
            let input = SkyModelInput {
                day: 0.5,
                realm_minute: 720,
                skyboxes: [(row.id(), opacity), (0, 0.), (0, 0.)],
                global_skybox: None,
                world_model: None,
                visible: true,
            };
            let prefix = if case == 0 { 7 } else { 19 };
            let (default_sky, frame) =
                sky.prepare_model_input(&mut renderer, camera, time, input, prefix, &mut random)?;
            assert_eq!(
                default_sky,
                opacity <= 0.99 || row.flags() & 2 != 0,
                "skybox {} replacement",
                row.id()
            );
            let count = frame.draw_count();
            assert_eq!(frame.glare_suppression(), opacity);
            assert!(count > 0, "skybox {} has no submitted batches", row.id());
            let scene = world_scene(camera).with_sky_models(frame);
            renderer.request_frame_capture()?;
            let report = renderer.present_world_frame(
                scene,
                &vec![Mat4::from_translation(Vec3::splat(5000.)); prefix],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            )?;
            assert_eq!(report.sky_model_draw_count(), count);
            let pixels = renderer
                .take_captured_frame()?
                .ok_or("missing skybox capture")?
                .rgba8()
                .to_vec();
            if case == 0 {
                let visible = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[..3].iter().any(|v| *v > 4))
                    .count();
                eprintln!("skybox={} visible={} draws={count}", row.id(), visible);
                visible_models += usize::from(visible > 50);
                original = pixels.clone();
                if let Some(directory) = std::env::var_os("SOLARITY_SKYBOX_CAPTURE_DIR") {
                    std::fs::write(
                        std::path::Path::new(&directory).join(format!("skybox-{}.rgba", row.id())),
                        &pixels,
                    )?;
                }
            } else if case == 1 {
                assert_eq!(
                    pixels,
                    original,
                    "skybox {} moved with the camera",
                    row.id()
                );
            } else {
                fading_models += usize::from(pixels != original);
            }
        }
        models_checked += 1;
    }
    assert_eq!(
        sky.skyboxes.len(),
        48,
        "installed unique model coverage changed"
    );
    assert!(visible_models >= 40, "only {visible_models} visible models");
    assert!(
        fading_models >= 40,
        "only {fading_models} models responded to opacity"
    );

    // Same path, different DBC flags: the first request retains phase flags,
    // while the active DBC row still controls replacement versus overlay.
    let first = sky
        .resolve_skybox(38, 200000)?
        .ok_or("skybox 38")?
        .0
        .ok_or("skybox model")?;
    let overlay = sky.resolve_skybox(40, 200000)?.ok_or("skybox 40")?;
    assert_eq!(overlay.0, Some(first));
    assert_eq!(sky.skyboxes[first].phase.flags, 0);
    assert_eq!(overlay.1, 2);
    let camera =
        WorldCamera::stock(Vec3::ZERO, Vec3::new(1., 0., 0.5), Vec3::Z, 1000.).frame(1.)?;
    for (weights, expected_default) in [
        ([0.25, 0.5, 0.75], true),
        ([0.5, 1., 0.25], false),
        ([1., 0.5, 0.25], false),
    ] {
        let input = SkyModelInput {
            day: 0.5,
            realm_minute: 900,
            skyboxes: [(2, weights[0]), (38, weights[1]), (40, weights[2])],
            global_skybox: None,
            world_model: None,
            visible: true,
        };
        let (default_sky, frame) =
            sky.prepare_model_input(&mut renderer, camera, 200000, input, 5, &mut random)?;
        assert_eq!(default_sky, expected_default);
        assert_eq!(
            frame.glare_suppression(),
            weights.into_iter().fold(0_f32, f32::max)
        );
        let count = frame.draw_count();
        renderer.request_frame_capture()?;
        let report = renderer.present_world_frame(
            world_scene(camera).with_sky_models(frame),
            &[Mat4::IDENTITY; 5],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        )?;
        assert_eq!(report.sky_model_draw_count(), count);
        assert!(renderer.take_captured_frame()?.is_some());
    }

    // The Legion dome is the installed lit skybox. Its own scene descriptor
    // must reach the draw; the ordinary/stars zero-light bank darkens this mesh.
    let input = SkyModelInput {
        day: 0.5,
        realm_minute: 900,
        skyboxes: [(2, 1.), (0, 0.), (0, 0.)],
        global_skybox: None,
        world_model: None,
        visible: true,
    };
    let (_, frame) =
        sky.prepare_model_input(&mut renderer, camera, 201000, input, 0, &mut random)?;
    renderer.request_frame_capture()?;
    renderer.present_world_frame(
        world_scene(camera).with_sky_models(frame),
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
    let lit = renderer
        .take_captured_frame()?
        .ok_or("missing lit skybox")?
        .rgba8()
        .to_vec();
    let unlit = solarity_rendering::WorldSkyModelFrame::new(
        sky_scene(camera),
        &sky.bones,
        &[],
        &sky.skybox_draws[0],
    );
    renderer.request_frame_capture()?;
    renderer.present_world_frame(
        world_scene(camera).with_sky_models(unlit),
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
    let dark = renderer
        .take_captured_frame()?
        .ok_or("missing zero-light skybox")?;
    let changed = lit
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dark.rgba8().as_chunks::<4>().0)
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    assert!(
        changed > 1000,
        "authored skybox lights affected only {changed} pixels"
    );
    Ok(())
}

fn world_scene(camera: solarity_rendering::WorldCameraFrame) -> WorldFrameScene<'static> {
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
        sky_scene(camera),
    )
}
