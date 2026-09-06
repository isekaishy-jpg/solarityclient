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
    assert!(!movement.initial_contact_ready());
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
    assert!(!movement.initial_contact_ready());
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
    assert!(movement.initial_contact_ready());
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

    // One pump can contain press, motion, and release. Left drag changes the
    // durable ECS camera without turning the body; right drag turns both.
    let hold = |control, pressed, timestamp_ms| UiMovementCommand {
        action: UiMovementAction::Hold { control, pressed },
        timestamp_ms,
    };
    let settings = crate::application::player_camera::PlayerCameraMouseSettings {
        yaw_speed: 180.,
        pitch_speed: 90.,
        invert_yaw: false,
        invert_pitch: false,
    };
    movement.push(hold(UiMovementControl::Forward, false, 1200));
    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, true, 1200));
    movement.push_mouse_motion([100., 50.], settings, 1200);
    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, false, 1200));
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1200,
    )?;
    let world = gameplay.world().ok_or("world")?;
    assert_eq!(world.local_player_transform()?.orientation(), 0.);
    assert!(world.local_player_view()?.yaw_offset_radians().abs() > 0.3);
    assert!(world.local_player_view()?.pitch_radians() > 0.2);
    assert!(!movement.mouse_free_look());

    movement.push(hold(UiMovementControl::TurnOrAction, true, 1200));
    movement.push_mouse_motion([20., 0.], settings, 1200);
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1200,
    )?;
    let world = gameplay.world().ok_or("world")?;
    assert!(world.local_player_transform()?.orientation() > 5.5);
    assert_eq!(world.local_player_view()?.yaw_offset_radians(), 0.);
    assert!(movement.mouse_free_look());
    while writer.try_recv().is_ok() {}

    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, true, 1200));
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1400,
    )?;
    let paired = gameplay
        .world()
        .ok_or("world")?
        .local_player_transform()?
        .position();
    assert!(
        ((paired - moved).length() - 1.4).abs() < 0.001,
        "{moved:?} -> {paired:?}"
    );
    assert!(paired.y < moved.y);
    // Native pair acquisition aligns facing before resolving forward input.
    for kind in [
        WorldMovementKind::SetFacing,
        WorldMovementKind::StartForward,
    ] {
        assert!(
            matches!(writer.try_recv()?, WorldWriterCommand::Movement(message) if message.kind() == kind)
        );
    }
    movement.push(hold(UiMovementControl::TurnOrAction, false, 1400));
    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, false, 1400));
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1400,
    )?;
    assert!(!movement.mouse_free_look());
    let original_view = gameplay.world().ok_or("world")?.local_player_view()?;
    movement.push(UiMovementCommand {
        action: UiMovementAction::CameraZoom {
            inward: true,
            amount: 1.,
        },
        timestamp_ms: 1400,
    });
    for now in [1400, 1460, 1520] {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            now,
        )?;
    }
    let zoomed_view = gameplay.world().ok_or("world")?.local_player_view()?;
    assert!((original_view.distance() - zoomed_view.distance() - 0.9996).abs() < 0.00001);
    assert_eq!(zoomed_view.pitch_radians(), original_view.pitch_radians());
    assert_eq!(
        zoomed_view.yaw_offset_radians(),
        original_view.yaw_offset_radians()
    );

    // An idle left orbit persists. Starting movement then follows on both
    // axes; stopping cancels the default Smarter transition at its current view.
    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, true, 1520));
    movement.push_mouse_motion([100., 300.], settings, 1520);
    movement.push(hold(UiMovementControl::CameraOrSelectOrMove, false, 1520));
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1520,
    )?;
    let orbit = gameplay.world().ok_or("world")?.local_player_view()?;
    let facing = gameplay
        .world()
        .ok_or("world")?
        .local_player_transform()?
        .orientation();
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        1800,
    )?;
    assert_eq!(gameplay.world().ok_or("world")?.local_player_view()?, orbit);
    movement.push(hold(UiMovementControl::Forward, true, 1800));
    for now in [1800, 1900, 2000] {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            now,
        )?;
    }
    let following = gameplay.world().ok_or("world")?.local_player_view()?;
    assert!(following.yaw_offset_radians().abs() < orbit.yaw_offset_radians().abs());
    assert!(following.yaw_offset_radians().abs() > 0.01);
    assert!(following.pitch_radians() < orbit.pitch_radians());
    assert!(following.pitch_radians() > 0.6);
    assert_eq!(
        gameplay
            .world()
            .ok_or("world")?
            .local_player_transform()?
            .orientation(),
        facing
    );
    movement.push(hold(UiMovementControl::Forward, false, 2000));
    for now in [2000, 2100, 2400] {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            now,
        )?;
    }
    assert_eq!(
        gameplay.world().ok_or("world")?.local_player_view()?,
        following
    );
    // A changed script policy is read once, while identical revisions avoid
    // every numeric lookup. Never disables recentering on the next input edge.
    movement.refresh_camera_settings(7, |name| (name == "camerasmoothstyle").then_some(0.));
    movement.refresh_camera_settings(7, |_| panic!("unchanged camera settings were reread"));
    movement.push(hold(UiMovementControl::Forward, true, 2400));
    for now in [2400, 2500, 2800] {
        movement.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            Some([0.5, 2., 1.]),
            now,
        )?;
    }
    assert_eq!(
        gameplay.world().ok_or("world")?.local_player_view()?,
        following
    );

    // An explicitly airborne entry is already presentable. It must not wait
    // behind the loading card until its eventual landing.
    let world = gameplay.world.as_mut().ok_or("world")?;
    let state = world.movement_state(1).ok_or("movement")?;
    let context = WorldMovementContext {
        falling: Some(solarity_ecs::WorldMovementFall {
            vertical_speed: 0.,
            direction_sin: 0.,
            direction_cos: 1.,
            horizontal_speed: 0.,
        }),
        ..WorldMovementContext::default()
    };
    world.update_movement(1, WorldMovementState::new(0x1000, state.speeds(), context))?;
    movement = RuntimePlayerMovement::default();
    movement.service(
        &mut gameplay,
        &mut terrain,
        &objects,
        Some([0.5, 2., 1.]),
        2800,
    )?;
    assert!(movement.initial_contact_ready());
    assert!(
        gameplay
            .world()
            .ok_or("world")?
            .movement_state(1)
            .ok_or("movement")?
            .context()
            .falling
            .is_some()
    );
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
