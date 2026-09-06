//! Production movement service crosses gameplay, residency, ECS and writer ownership.

use super::*;
use crate::application::player_movement::RuntimePlayerMovement;
use crate::application::{RuntimeGameObjectPresentation, RuntimeTerrainCoordinator};
use crate::test_support::{ClientFixture, bootstrap_texture_blp};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot,
    GameObjectDisplayCatalog, Locale, MapCatalog,
};
use solarity_ecs::{WorldMovementContext, WorldMovementSpeeds, WorldMovementState};
use solarity_network::WorldMovementKind;
use solarity_ui::{UiMovementAction, UiMovementCommand, UiMovementControl};
use std::{error::Error, io::Cursor, sync::Arc};

#[test]
fn entry_resolves_support_and_moves_without_an_external_ground_ready_callback()
-> Result<(), Box<dyn Error>> {
    let fixture = flat_world()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets.clone(), maps);
    let objects = RuntimeGameObjectPresentation::new(assets, displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Entry",
        Vec3::new(1000., 5800., 11.),
        0.,
    ));
    world.update_movement(
        1,
        WorldMovementState::new(
            0,
            WorldMovementSpeeds::new([
                2.5,
                7.,
                4.5,
                4.72,
                2.5,
                7.,
                4.5,
                std::f32::consts::PI,
                std::f32::consts::PI,
            ]),
            WorldMovementContext::default(),
        ),
    )?;
    let player = world.local_player();
    world.storage_mut().add_component(
        player,
        (solarity_ecs::UnitVitals::new(100, 100, [0; 7], [0; 7]),),
    );
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (_packets, receiver) = mpsc::channel(8);
    let (commands, mut writer) = mpsc::channel(128);
    let mut gameplay = RuntimeGameplayCoordinator::new();
    gameplay.world = Some(world);
    gameplay.active = Some(ActiveGameplayNetwork {
        receiver,
        commands,
        task: runtime.spawn(std::future::pending()),
    });
    let mut movement = RuntimePlayerMovement::default();
    // Model readiness arrives after entry. No independent ground-ready flag
    // or direct position write is supplied to this production service.
    movement.service(&mut gameplay, &mut terrain, &objects, None, 0)?;
    assert!(writer.try_recv().is_err());
    for time in [0, 100] {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            time,
        )?;
    }
    assert!(matches!(
        writer.try_recv()?,
        WorldWriterCommand::ActiveMover(1)
    ));
    assert!(
        matches!(writer.try_recv()?, WorldWriterCommand::Movement(message)
        if message.kind() == WorldMovementKind::Heartbeat)
    );
    assert_eq!(
        gameplay
            .world()
            .ok_or("world")?
            .local_player_transform()?
            .position()
            .z,
        11.
    );
    terrain.synchronize(gameplay.world())?;
    terrain.synchronize_game_object_movement(
        gameplay.world(),
        &objects,
        solarity_systems::MovementBspCacheMode::Enabled,
    )?;
    for time in (150..=1000).step_by(50) {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            time,
        )?;
    }
    let landed = gameplay
        .world()
        .ok_or("world")?
        .local_player_transform()?
        .position();
    assert!((landed.z - 10.).abs() < 0.01, "{landed:?}");
    movement.push(UiMovementCommand {
        action: UiMovementAction::Hold {
            control: UiMovementControl::Forward,
            pressed: true,
        },
        timestamp_ms: 1000,
    });
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1000,
    )?;
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1200,
    )?;
    let moved = gameplay
        .world()
        .ok_or("world")?
        .local_player_transform()?
        .position();
    assert!(
        (moved.x - landed.x - 1.4).abs() < 0.001,
        "{landed:?} -> {moved:?}"
    );
    assert!((moved.z - 10.).abs() < 0.01);
    let mut kinds = Vec::new();
    while let Ok(command) = writer.try_recv() {
        if let WorldWriterCommand::Movement(message) = command {
            kinds.push(message.kind());
        }
    }
    assert!(kinds.contains(&WorldMovementKind::FallLand));
    assert!(kinds.contains(&WorldMovementKind::StartForward));
    Ok(())
}

fn flat_world() -> Result<ClientFixture, Box<dyn Error>> {
    let mut map = [0_u32; 66];
    map[0] = 571;
    map[1] = 1;
    map[5] = 1;
    map[22] = 571;
    map[59] = u32::MAX;
    map[63] = 2;
    let maps = dbc(66, &map, b"\0Northrend\0");
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    manifest
        .main
        .get_mut(21, 30)
        .ok_or("tile")?
        .set_has_adt(true);
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let bytes = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut Cursor::new(bytes))? else {
        return Err("not a root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (30 * 16 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (21 * 16 + chunk.header.index_x) as f32 * 33.333_332,
            10.,
        ];
        chunk.heights.as_mut().ok_or("heights")?.heights.fill(0.);
    }
    let adt = wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &maps),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &dbc(19, &[], b"\0"),
        ),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        ("World\\Maps\\Northrend\\Northrend_21_30.adt", &adt),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
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
