//! Synthetic authored skyboxes verify WMO replacement, fade and hidden clocks.

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use std::error::Error;

#[test]
#[allow(unsafe_code)] // Hidden SDL window transfers surface ownership to Vulkan.
fn world_model_skybox_preserves_cached_phase_and_pauses_hidden_scene() -> Result<(), Box<dyn Error>>
{
    let mut model = game_object_models::model_with_animations(&[0])?;
    let vertices = u32::from_le_bytes(model[0x40..0x44].try_into()?) as usize;
    for (i, point) in [[5f32, -4., -4.], [5., 4., -4.], [5., 0., 4.]]
        .into_iter()
        .enumerate()
    {
        for (axis, value) in point.into_iter().enumerate() {
            model[vertices + i * 48 + axis * 4..vertices + i * 48 + axis * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    let material = u32::from_le_bytes(model[0x74..0x78].try_into()?) as usize;
    model[material..material + 2].copy_from_slice(&0x17u16.to_le_bytes());
    model[material + 2..material + 4].copy_from_slice(&2u16.to_le_bytes());
    let skin = game_object_models::skin()?;
    let definitions = skybox_table();
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient/LightSkybox.dbc", &definitions),
        ("A.m2", &model),
        ("A00.skin", &skin),
        ("B.m2", &model),
        ("B00.skin", &skin),
        ("C.m2", &model),
        ("C00.skin", &skin),
    ])?;
    let mut store = solarity_asset::AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    let animations = Arc::new(solarity_asset::AnimationDataCatalog::load(&mut store)?);
    let store = AssetStoreHandle::new(store);
    let mut sky = RuntimeSkyResources::load(store, &lights, animations)?;
    let _lock = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity WMO sky scene", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: SDL transfers sole ownership; the window outlives the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 1000.).frame(1.)?;
    let mut random = crate::random::CrtRand::new();
    let mut full_pixels = Vec::new();
    // Hidden frames still request palette/MOSB models. They do not start their
    // animation, alter phase, or consume the process random stream.
    for (step, visible, minute, opacity, wmo) in [
        (0, false, 0, 1., true),
        (1, true, 40, 1., true),
        (2, false, 80, 0.5, true),
        (3, true, 90, 0.5, true),
        (4, true, 91, 1., false),
    ] {
        let before_random = random;
        let before_phase = sky
            .skyboxes
            .iter()
            .map(|entry| (entry.phase.duration, entry.phase.last_minute))
            .collect::<Vec<_>>();
        let input = SkyModelInput {
            day: 0.5,
            realm_minute: minute,
            skyboxes: [(1, 0.2), (2, 0.3), (3, 0.4)],
            world_model: wmo.then_some(("b.m2", opacity)),
            visible,
        };
        let (default_sky, frame) =
            sky.prepare_model_input(&mut renderer, camera, step * 1000, input, 0, &mut random)?;
        assert_eq!(default_sky, visible && (!wmo || opacity <= 0.99));
        assert_eq!(
            frame.draw_count(),
            if !visible {
                0
            } else if wmo {
                2
            } else {
                3
            }
        );
        renderer.request_frame_capture()?;
        let scene = world_scene(camera)
            .with_sky_models(frame)
            .with_sky_window(visible.then_some(solarity_rendering::WorldSkyWindow::FULL));
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(
            report.sky_model_draw_count(),
            if !visible {
                0
            } else if wmo {
                2
            } else {
                3
            }
        );
        let capture = renderer.take_captured_frame()?.ok_or("missing sky frame")?;
        if !visible {
            assert_eq!(random, before_random);
            assert!(
                capture
                    .rgba8()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[..3] == [0, 0, 0])
            );
            if !before_phase.is_empty() {
                assert_eq!(
                    sky.skyboxes
                        .iter()
                        .map(|entry| (entry.phase.duration, entry.phase.last_minute))
                        .collect::<Vec<_>>(),
                    before_phase
                );
            }
        } else if step == 1 {
            full_pixels = capture.rgba8().to_vec();
            assert!(
                full_pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[0] > 128)
                    .count()
                    > 500
            );
        } else if step == 3 {
            let changed = full_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(capture.rgba8().as_chunks::<4>().0)
                .filter(|(a, b)| a[..3] != b[..3])
                .count();
            assert!(changed > 500, "WMO fade changed only {changed} pixels");
        }
        assert_eq!(sky.skyboxes.len(), 3); // Case alias reuses B, including its first flags.
        assert_eq!(sky.skyboxes[1].phase.flags, 3);
        if visible {
            assert_eq!(sky.skyboxes[1].phase.last_minute, minute);
        }
    }
    Ok(())
}

/// Three overlay slots preserve authored order until the WMO replaces 0/1.
fn skybox_table() -> Vec<u8> {
    let strings = b"\0A.m2\0B.m2\0C.m2\0";
    let words = [
        3u32,
        3,
        12,
        strings.len() as u32,
        1,
        1,
        2,
        2,
        6,
        3,
        3,
        11,
        2,
    ];
    let mut bytes = b"WDBC".to_vec();
    bytes.extend(words.into_iter().flat_map(u32::to_le_bytes));
    bytes.extend_from_slice(strings);
    bytes
}
