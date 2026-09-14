//! Decoded floor and MODD lighting reach actual M2 frame instance uniforms.
use super::*;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use crate::test_support::registration_fixture::fixture_files;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models};
use solarity_asset::{AssetStoreHandle, MapCatalog};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};

#[path = "world_model_doodad_frame.rs"]
mod doodad_frame;
#[path = "model_liquid_frame.rs"]
mod model_liquid_frame;

#[test]
fn authored_terrain_shadow_reaches_retained_entity_lighting() -> Result<(), Box<dyn Error>> {
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    manifest
        .main
        .get_mut(32, 32)
        .ok_or("tile")?
        .set_has_adt(true);
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let adt = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut std::io::Cursor::new(adt))?
    else {
        return Err("root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (512 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (512 + chunk.header.index_x) as f32 * 33.333_332,
            0.,
        ];
        chunk.header.flags.value |= 1;
        chunk.header.size_shadow = 512;
        chunk.shadow = Some(wow_adt::McshChunk {
            shadow_map: (0..512)
                .map(|byte| if byte % 8 < 4 { 255 } else { 0 })
                .collect(),
        });
        chunk.heights.as_mut().ok_or("heights")?.heights.fill(0.);
    }
    let adt = wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &fixture_files().3),
        ("World\\Maps\\Light\\Light.wdt", &wdt),
        ("World\\Maps\\Light\\Light_32_32.adt", &adt),
        (
            "tileset\\fixture\\grass.blp",
            &crate::test_support::bootstrap_texture_blp(),
        ),
        ("Receiver.m2", &game_object_models::model()?),
        ("Receiver00.skin", &game_object_models::skin()?),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Receiver.m2")?,
    )?);
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let shadowed = Vec3::new(-4., -4., 1.);
    let clear = Vec3::new(-4., -24., 1.);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Light",
        shadowed,
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    let mut light = entity_lighting::EntityLighting::default();
    let environment = solarity_systems::WorldEntityLightEnvironment::new(
        Vec3::splat(0.2),
        Vec3::splat(0.8),
        -Vec3::Z,
        -Vec3::Z,
    );
    // Ordinary terrain doodads have no individual light callback. A later first
    // entity sample must still start its transition clock at that sample's time.
    assert!(
        light
            .sample(
                M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad { unique_id: 1 }),
                &model,
                Mat4::from_translation(shadowed),
                [255; 4],
                0.,
                &mut terrain,
                environment,
            )?
            .is_none()
    );
    for (time, position, gain) in [
        (1000., shadowed, 1.),
        (2000., shadowed, 0.5),
        (3000., clear, 1.),
        (4000., shadowed, 0.5),
    ] {
        let sample = light
            .sample(
                M2GpuPlacementOwner::CreatureBody { guid: 99 },
                &model,
                Mat4::from_translation(position),
                [255; 4],
                time,
                &mut terrain,
                environment,
            )?
            .ok_or("entity callback")?;
        assert!(
            (sample.diffuse() - Vec3::splat(0.8 * gain))
                .abs()
                .max_element()
                < 0.000001,
            "time {time}"
        );
    }
    terrain.disconnect();
    let sample = light
        .sample(
            M2GpuPlacementOwner::CreatureBody { guid: 99 },
            &model,
            Mat4::from_translation(shadowed),
            [255; 4],
            5000.,
            &mut terrain,
            environment,
        )?
        .ok_or("disconnected callback")?;
    assert!((sample.diffuse() - Vec3::splat(0.8)).abs().max_element() < 0.000001);
    Ok(())
}

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
    let liquid_types = solarity_asset::LiquidTypeCatalog::load(&mut store)?;
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
    let (interior, floor, terrain_shadow) =
        terrain.model_floor_light(unit_position, None, &mut scratch)?;
    assert!(interior);
    assert!(!terrain_shadow);
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
    // Keep the camera registered over this fixture's interior floor. Its WMO
    // has no exterior entry, so a camera outside the room cannot see its MODD.
    let camera = WorldCamera::stock(
        unit_position + Vec3::new(0.5, 0., 1.),
        unit_position,
        Vec3::Z,
        100.,
    )
    .frame(1.)?;
    let base = M2SceneUniform::new(
        camera.projection(),
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
    for now in [0., 1000., 1500.] {
        if now == 1500. {
            let opacity =
                Rc::new(crate::application::entity_opacity::EntityOpacityOwner::default());
            opacity.select_model(1, 1., 0, 1000);
            opacity.mark_removed(1000);
            frame.placements[1].entity_opacity = Some(opacity);
            frame.retire_removed_models();
        }
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            None,
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
            Some((&mut terrain, environment, Vec3::ZERO, &liquid_types)),
            None,
            None,
        )?;
        assert_eq!(visible.draws.len(), 2);
        assert_ne!(visible.instance_scenes[0], visible.instance_scenes[1]);
        if now >= 1000. {
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
        if now == 1500. {
            assert!(
                visible
                    .draws
                    .iter()
                    .any(|draw| (draw.material().alpha() - 0.84375).abs() < 0.00001)
            );
            assert!(
                frame.placements[1].retirement.is_some(),
                "the retained floor callback also serves the detached owner"
            );
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
