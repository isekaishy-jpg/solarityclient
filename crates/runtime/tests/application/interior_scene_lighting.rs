//! Decoded floor and MODD lighting reach actual M2 frame instance uniforms.
use super::*;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models};
use solarity_asset::{AssetStoreHandle, MapCatalog};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};

#[test]
#[allow(unsafe_code)]
fn interior_floor_and_doodad_lights_reach_model_uniforms() -> Result<(), Box<dyn Error>> {
    let (root, group, wdt, map) = fixture_files();
    let model_bytes = game_object_models::model()?;
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map),
        ("World\\Maps\\Light\\Light.wdt", &wdt),
        ("World\\Light.wmo", &root),
        ("World\\Light_000.wmo", &group),
        ("Receiver.m2", &model_bytes),
        ("Receiver00.skin", &skin),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Receiver.m2")?,
    )?);
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Light",
        Vec3::ZERO,
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    assert_eq!(terrain.resident_m2_count(), 1);
    let root_transform = terrain
        .resident_m2_scene()
        .ok_or("resident doodad")?
        .placements()[0]
        .transform();
    let unit_transform = root_transform * Mat4::from_translation(Vec3::new(1., 1., 0.));
    let unit_position = unit_transform.w_axis.truncate();
    let mut scratch =
        crate::application::terrain_coordinator::RuntimeMovementRegistrationQuery::new();
    let (interior, floor) = terrain.model_floor_light(unit_position, None, &mut scratch)?;
    assert!(interior);
    let floor = floor.ok_or("floor color")?;
    let native =
        include_bytes!("../../../systems/tests/fixtures/world_model_floor_light_native.bin")
            .as_chunks::<92>()
            .0[38];
    assert_eq!(floor.color(), native[76..80]);
    assert!(!floor.blends_exterior());

    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity interior light frame", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live window owns the surface until ownership transfers to the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        terrain.resident_m2_scene().ok_or("M2 scene")?,
        Arc::clone(&animations),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
    frame.placements.push(m2_gpu_placement(
        0,
        unit_transform,
        M2GpuPlacementOwner::CreatureBody { guid: 1 },
        &model,
        Some(M2PlaybackStorage::Local(playback)),
        None,
        0,
    )?);
    frame.placement_topology_dirty = true;
    let camera = WorldCamera::stock(
        unit_position + Vec3::new(8., 0., 4.),
        unit_position,
        Vec3::Z,
        100.,
    )
    .frame(1.)?;
    let base = M2SceneUniform::new(
        camera.view_projection(),
        camera.view(),
        camera.camera().position(),
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    let exterior = M2DirectionalLight::new(-Vec3::Z, Vec3::splat(0.8), Vec3::splat(0.9));
    let environment = solarity_systems::WorldEntityLightEnvironment::new(
        exterior.ambient(),
        exterior.diffuse(),
        exterior.direction(),
        exterior.direction(),
    );
    for now in [0., 1000.] {
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            now,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            Some((base, exterior)),
            Some((&mut terrain, environment)),
        )?;
        assert_eq!(visible.draws.len(), 2);
        assert_ne!(visible.instance_scenes[0], visible.instance_scenes[1]);
        if now == 1000. {
            let bytes = visible.instance_scenes[1].to_bytes();
            let expected = &native[84..88];
            for channel in 0..3 {
                assert!(
                    (word(&bytes, 176 + channel * 4) - f32::from(expected[2 - channel]) / 255.)
                        .abs()
                        < 0.00001
                );
            }
        }
    }
    assert_eq!(
        placement_mesh_color(frame.placements[0].owner, [1, 2, 3, 0]),
        Vec4::ONE
    );
    assert_eq!(frame.placements[0].color, [0x40, 0x30, 0x20, 0x10]);
    let revision = terrain.model_light_revision();
    terrain.disconnect();
    assert_ne!(revision, terrain.model_light_revision());
    Ok(())
}

fn fixture_files() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        bytes.extend(tag);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
    }
    let mut header = vec![0; 64];
    for (at, value) in [(4, 1), (16, 1), (20, 1), (24, 1), (28, 0x743b2511), (60, 8)] {
        put(&mut header, at, value);
    }
    for (at, value) in [
        (36, -10f32),
        (40, -10.),
        (44, -10.),
        (48, 10.),
        (52, 10.),
        (56, 10.),
    ] {
        put(&mut header, at, value.to_bits());
    }
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17u32.to_le_bytes());
    chunk(&mut root, b"DHOM", &header);
    let mut info = vec![0; 32];
    info[4..28].copy_from_slice(&header[36..60]);
    put(&mut info, 28, u32::MAX);
    chunk(&mut root, b"IGOM", &info);
    chunk(&mut root, b"NDOM", b"Receiver.mdx\0");
    let mut doodad = [0; 40];
    put(&mut doodad, 28, 1f32.to_bits());
    put(&mut doodad, 32, 1f32.to_bits());
    put(&mut doodad, 36, 0x10203040);
    chunk(&mut root, b"DDOM", &doodad);
    let mut set = [0; 32];
    set[..18].copy_from_slice(b"Set_$DefaultGlobal");
    put(&mut set, 24, 1);
    chunk(&mut root, b"SDOM", &set);
    let mut group_header = vec![0; 68];
    put(&mut group_header, 8, 0x801);
    group_header[12..36].copy_from_slice(&header[36..60]);
    // 0x20 participates in both native primary and floor-color channels.
    chunk(&mut group_header, b"YPOM", &[0x20, 255]);
    chunk(&mut group_header, b"IVOM", &[0, 0, 1, 0, 2, 0]);
    chunk(
        &mut group_header,
        b"TVOM",
        &floats(&[0., 0., 0., 4., 0., 0., 0., 4., 0.]),
    );
    chunk(
        &mut group_header,
        b"RNOM",
        &floats(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]),
    );
    let mut node = [0; 16];
    node[..8].copy_from_slice(&[4, 0, 255, 255, 255, 255, 1, 0]);
    chunk(&mut group_header, b"NBOM", &node);
    chunk(&mut group_header, b"RBOM", &[0, 0]);
    chunk(
        &mut group_header,
        b"VCOM",
        &[0xff102030u32, 0x40205070, 0x8090a0b0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    chunk(&mut group_header, b"RDOM", &[0, 0]);
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17u32.to_le_bytes());
    chunk(&mut group, b"PGOM", &group_header);
    let mut wdt = Vec::new();
    chunk(&mut wdt, b"REVM", &18u32.to_le_bytes());
    let mut mphd = [0; 32];
    put(&mut mphd, 0, 1);
    chunk(&mut wdt, b"DHPM", &mphd);
    chunk(&mut wdt, b"NIAM", &vec![0; 32768]);
    chunk(&mut wdt, b"OMWM", b"World\\Light.wmo\0");
    let mut modf = [0; 64];
    put(&mut modf, 4, 7);
    chunk(&mut wdt, b"FDOM", &modf);
    let mut fields = [0u32; 66];
    fields[0] = 571;
    fields[1] = 1;
    fields[5] = 1;
    fields[22] = 571;
    fields[59] = u32::MAX;
    let mut map = b"WDBC".to_vec();
    map.extend([1u32, 66, 264, 7].into_iter().flat_map(u32::to_le_bytes));
    map.extend(fields.into_iter().flat_map(u32::to_le_bytes));
    map.extend(b"\0Light\0");
    (root, group, wdt, map)
}
