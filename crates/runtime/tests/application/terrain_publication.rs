//! Recoverable unit errors cannot strand an accepted CPU terrain generation.

use super::*;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, bootstrap_texture_blp};
use glam::Vec3;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};
use std::{
    error::Error,
    ffi::OsString,
    time::{Duration, Instant},
};

#[test]
fn terrain_publication_precedes_recoverable_unit_errors() -> Result<(), Box<dyn Error>> {
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = fixture()?;
    let mut arguments: Vec<OsString> = [
        "--locale",
        "enUS",
        "--cpu-workers",
        "2",
        "--cpu-capacity",
        "8",
        "--network-workers",
        "1",
        "--network-shutdown-ms",
        "250",
        "--login-endpoint",
        "127.0.0.1:3724",
        "--login-timezone-minutes",
        "-240",
        "--login-client-ip",
        "127.0.0.1",
        "--window-width",
        "320",
        "--window-height",
        "240",
        "--window-mode",
        "windowed",
        "--gpu-index",
        "0",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.extend([
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--profile-root"),
        fixture.profile_root().as_os_str().to_owned(),
    ]);
    let configuration = RuntimeConfiguration::from_arguments(arguments)?;
    let (mut services, _, _) = ClientServices::start(&configuration)?;
    services.gameplay = RuntimeGameplayCoordinator::with_test_world(world(1000., false)?);
    let deadline = Instant::now() + Duration::from_secs(10);
    while services.terrain_frame.is_none() {
        services.service_login()?;
        assert!(Instant::now() < deadline, "initial terrain timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    let initial = services.terrain_frame.as_ref().and_then(TerrainFrame::tile);
    assert_eq!(initial.map(|tile| (tile.x(), tile.y())), Some((21, 30)));

    // A new neighboring unit fails presentation while the player's tile loads.
    // This used to skip GPU publication on every frame, including TileLoaded.
    services.gameplay = RuntimeGameplayCoordinator::with_test_world(world(500., true)?);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let error = services
            .service_login()
            .err()
            .ok_or("invalid NPC must report a presentation error")?;
        assert!(services.record_recoverable_error(&error), "{error}");
        let tile = services
            .terrain
            .resident_mesh_plan()
            .map(|plan| plan.tile());
        if tile != initial {
            assert_eq!(tile.map(|tile| (tile.x(), tile.y())), Some((21, 31)));
            assert_eq!(
                services.terrain_frame.as_ref().and_then(TerrainFrame::tile),
                tile
            );
            break;
        }
        assert!(Instant::now() < deadline, "neighbor terrain timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    // The following Current result must keep the same consistent generation.
    let error = services
        .service_login()
        .err()
        .ok_or("invalid NPC still reports its own error")?;
    assert!(services.record_recoverable_error(&error));
    assert_eq!(
        services.terrain_frame.as_ref().and_then(TerrainFrame::tile),
        services
            .terrain
            .resident_mesh_plan()
            .map(|plan| plan.tile())
    );
    services.shutdown()?;
    Ok(())
}

fn world(x: f32, invalid_npc: bool) -> Result<ActiveWorld, Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Test",
        Vec3::new(x, 5800., 10.),
        0.,
    ));
    if invalid_npc {
        let fields = [
            (4, 1.0_f32.to_bits()),
            (23, u32::from_le_bytes([9, 1, 0, 0])),
            (67, 999_999),
            (68, 999_999),
        ];
        world.create_object(
            2,
            ObjectKind::Unit,
            Some(WorldTransform::new(Vec3::new(x, 5800., 10.), 0.)),
            fields,
        )?;
        solarity_systems::project_object_fields(&mut world, 2, fields)?;
    }
    Ok(world)
}

fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    let mut map = [0; 66];
    map[0] = 571;
    map[1] = 1;
    map[5] = 1;
    map[22] = 571;
    map[59] = u32::MAX;
    map[63] = 2;
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    for y in [30, 31] {
        manifest
            .main
            .get_mut(21, y)
            .ok_or("tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let bytes = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut std::io::Cursor::new(bytes))?
    else {
        return Err("not a root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    let adt = wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    ClientFixture::with_common_files(&[
        ("DBFilesClient\\Spell.dbc", &dbc(234, &[], b"\0")),
        ("DBFilesClient\\Map.dbc", &dbc(66, &map, b"\0Northrend\0")),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &dbc(19, &[], b"\0"),
        ),
        ("DBFilesClient\\UISoundLookups.dbc", &dbc(3, &[], b"\0")),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        ("World\\Maps\\Northrend\\Northrend_21_30.adt", &adt),
        ("World\\Maps\\Northrend\\Northrend_21_31.adt", &adt),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        // MTXF zero selects the masked variant with the default specular CVar.
        ("tileset\\fixture\\grass_s.blp", &bootstrap_texture_blp()),
    ])
}

fn dbc(fields: u32, values: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        values.len() as u32 / fields,
        fields,
        fields * 4,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// A complete worker result remains outside collision ownership while GPU admission is pending.
#[test]
fn neighbor_terrain_waits_for_exact_gpu_admission_before_cpu_publication()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog = solarity_asset::ArchiveCatalog::discover(
        solarity_asset::ClientDataRoot::new(fixture.data_root())?,
        solarity_asset::Locale::EnUs,
    )?;
    let mut store = solarity_asset::AssetStore::mount(catalog.clone())?;
    let maps = solarity_asset::MapCatalog::load(&mut store)?;
    let mut terrain = crate::application::RuntimeTerrainCoordinator::new(
        solarity_asset::AssetStoreHandle::new(store),
        maps,
    )
    .with_worker_catalog(catalog);
    let world = world(1000., false)?;
    terrain.synchronize(Some(&world))?;
    let origin = Vec3::new(1000., 5800., 10.);
    let distance = solarity_systems::resolve_world_view_distance(
        solarity_systems::WorldViewDistanceRequest::new(777., WorldMapId::new(571), 0x8000_0000),
    )?;
    let window =
        solarity_systems::TerrainStreamingWindow::new(origin, distance, Vec3::new(777., 0., 0.))?;
    let mut cpu = solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        std::num::NonZeroUsize::MIN,
        std::num::NonZeroUsize::new(2).ok_or("capacity")?,
    ))?;
    terrain.synchronize_streaming_with_admission(571, origin, window, &cpu, |_| Ok(false))?;
    cpu.try_submit(|| ())?.join()?;
    let neighbor = solarity_asset::TerrainTileIndex::new(21, 31).ok_or("neighbor")?;
    let mut offered = 0;
    for _ in 0..3 {
        terrain.synchronize_streaming_with_admission(571, origin, window, &cpu, |tile| {
            assert_eq!(tile.mesh().tile(), neighbor);
            offered += 1;
            Ok(false)
        })?;
        assert!(terrain.resident_tile_at(neighbor).is_none());
        assert_eq!(terrain.resident_tile_count(), 1);
    }
    assert_eq!(
        offered, 3,
        "the same complete generation waits without reloading"
    );
    terrain.synchronize_streaming_with_admission(571, origin, window, &cpu, |_| Ok(true))?;
    assert!(terrain.resident_tile_at(neighbor).is_some());
    assert_eq!(terrain.resident_tile_count(), 2);
    cpu.shutdown()?;
    Ok(())
}
