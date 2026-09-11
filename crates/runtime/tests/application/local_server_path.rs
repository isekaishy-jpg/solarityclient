//! Server-forced local travel preserves controls and emits native completion.

use super::*;
use crate::application::player_movement::tests::{Floor, owner};
use solarity_network::{MonsterMove, MonsterMovePath, MovementSplineFacing};
use solarity_ui::{UiMovementAction, UiMovementControl};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn local_path_completion_waits_for_passenger_exit_before_resuming_held_input() -> TestResult {
    use crate::application::{
        unit_animation::UnitAnimationScene, unit_passenger::UnitPassengerFrames,
    };
    use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, VehicleCatalog};
    use solarity_ecs::{ObjectKind, WorldMovementTransport};
    use std::sync::Arc;

    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let vehicles = Arc::new(VehicleCatalog::load(&mut store)?);
    let frames = UnitPassengerFrames::new(Arc::clone(&vehicles));
    let (mut world, mut owner) = alive_world()?;
    world.create_object(
        9,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [(4, 1_f32.to_bits())],
    )?;
    world.set_unit_vehicle(9, 1, 0.);
    let mut context = owner.snapshot().1.context();
    context.transport = Some(WorldMovementTransport {
        guid: 9,
        position: Vec3::ZERO,
        orientation: 0.,
        time_ms: 0,
        seat: 2,
        interpolated_time_ms: None,
    });
    let old = WorldMovementState::new(0x200, owner.speeds, context);
    world.update_movement(1, old)?;
    let mut scene = UnitAnimationScene::default();
    let mut input = PlayerInputState::default();
    let mut output = VecDeque::new();
    let mut floor = Floor::new()?;
    let mut exit = path(1, 91, [0.; 3], [2., 0., 0.]);
    exit.path.as_mut().ok_or("path")?.duration_ms = 100;
    owner.receive_server_path(&exit, 0, 1., &mut floor, &mut output)?;
    assert_eq!(
        owner
            .snapshot()
            .1
            .spline()
            .ok_or("exit spline")?
            .duration_ms,
        100
    );
    owner.notify_server_parent((WorldTransform::new(Vec3::ZERO, 0.), old), None, true);
    owner.synchronize_passenger_input(&world, &frames, &scene);
    assert!(
        scene.passenger_input_blocked(owner.identity),
        "receipt must admit exit before ECS publication"
    );
    assert!(
        world
            .movement_state(1)
            .ok_or("old ECS")?
            .transport_guid()
            .is_some()
    );
    owner.command(
        MovementCommand::Input(UiMovementCommand {
            action: UiMovementAction::Hold {
                control: UiMovementControl::Forward,
                pressed: true,
            },
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    world.update_movement(1, owner.snapshot().1)?;
    scene.synchronize_passengers(&world, &vehicles, &frames);
    owner.advance_to(100, [0.5, 2., 1.], &mut floor, &mut output)?;
    owner.synchronize_passenger_input(&world, &frames, &scene);
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    assert!(matches!(
        output.back(),
        Some(PlayerMovementOutput::SplineDone { path_id: 91, .. })
    ));
    assert!(!owner.admission(&world).translation);
    assert!(!owner.admission(&world).turning);
    assert_eq!(input.held_bits() & 0x10010, 0x10);
    owner.advance_to(125, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position.x, 2.);
    world.update_transform(1, owner.snapshot().0)?;
    scene.set_scene_time(125);
    scene.synchronize_passengers(&world, &vehicles, &frames);
    assert!(
        scene.passenger_input_blocked(owner.identity),
        "travel phase still blocks input"
    );
    scene.set_scene_time(625);
    scene.synchronize_passengers(&world, &vehicles, &frames);
    assert!(!scene.passenger_input_blocked(owner.identity));
    owner.synchronize_passenger_input(&world, &frames, &scene);
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    assert!(
        matches!(output.back(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::StartForward)
    );
    owner.advance_to(225, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((owner.position.x - 2.7).abs() < 0.0001);
    Ok(())
}

fn path(guid: u64, id: u32, start: [f32; 3], end: [f32; 3]) -> MonsterMove {
    MonsterMove {
        guid,
        transport: None,
        control_byte: 0,
        start,
        id,
        facing_type: 0,
        facing: MovementSplineFacing::Direction,
        path: Some(MonsterMovePath {
            flags: 0,
            duration_ms: 1000,
            animation: None,
            parabolic: None,
            points: vec![end],
        }),
    }
}

fn alive_world() -> Result<(ActiveWorld, LocalMovement), Box<dyn std::error::Error>> {
    let (mut world, owner) = owner()?;
    let entity = world.local_player();
    world.storage_mut().add_component(
        entity,
        (solarity_ecs::UnitVitals::new(100, 100, [0; 7], [0; 7]),),
    );
    Ok((world, owner))
}

#[test]
fn local_path_blocks_input_then_acknowledges_before_resuming_held_forward() -> TestResult {
    let (world, mut owner) = alive_world()?;
    let mut input = PlayerInputState::default();
    let mut output = VecDeque::new();
    let mut floor = Floor::new()?;
    owner.command(
        MovementCommand::Input(UiMovementCommand {
            action: UiMovementAction::Hold {
                control: UiMovementControl::Forward,
                pressed: true,
            },
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    output.clear();
    let camera = owner.camera.view(owner.world_orientation());
    owner.receive_server_path(
        &path(1, 73, [0.; 3], [10., 0., 0.]),
        0,
        1.,
        &mut floor,
        &mut output,
    )?;
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    assert!(output.is_empty(), "path admission must not send input Stop");
    assert_eq!(input.held_bits() & 0x10010, 0x10);
    assert_eq!(owner.camera.view(owner.world_orientation()), camera);
    assert!(!owner.admission(&world).translation);
    assert!(!owner.admission(&world).turning);
    owner.advance_to(500, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((owner.position.x - 5.).abs() < 0.0001);
    assert!(owner.path_active());
    output.clear();
    owner.advance_to(1000, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, Vec3::new(10., 0., 0.));
    assert!(!owner.path_active());
    assert!(matches!(
        output.front(),
        Some(PlayerMovementOutput::SplineDone { path_id: 73, .. })
    ));
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    assert_eq!(output.len(), 2);
    assert!(
        matches!(output.back(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::StartForward)
    );
    owner.advance_to(1100, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((owner.position.x - 10.7).abs() < 0.0001);
    assert_eq!(
        output
            .iter()
            .filter(|event| matches!(event, PlayerMovementOutput::SplineDone { .. }))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn replacement_path_retires_old_geometry_without_acknowledging_it() -> TestResult {
    let (world, mut owner) = alive_world()?;
    let mut input = PlayerInputState::default();
    let mut floor = Floor::new()?;
    let mut output = VecDeque::new();
    owner.receive_server_path(
        &path(1, 3, [0.; 3], [10., 0., 0.]),
        0,
        1.,
        &mut floor,
        &mut output,
    )?;
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    owner.advance_to(250, [0.5, 2., 1.], &mut floor, &mut output)?;
    let start = owner.position.to_array();
    owner.receive_server_path(
        &path(1, 4, start, [2.5, 10., 0.]),
        250,
        1.,
        &mut floor,
        &mut output,
    )?;
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    owner.advance_to(1250, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, Vec3::new(2.5, 10., 0.));
    let completions: Vec<_> = output
        .iter()
        .filter_map(|event| match event {
            PlayerMovementOutput::SplineDone { path_id, .. } => Some(*path_id),
            _ => None,
        })
        .collect();
    assert_eq!(completions, [4]);
    Ok(())
}

#[test]
fn inactive_local_player_still_follows_server_path_without_sending_packets() -> TestResult {
    let (_, mut owner) = alive_world()?;
    let mut floor = Floor::new()?;
    let mut output = VecDeque::new();
    owner.active = false;
    owner.scene_collision = true;
    owner.receive_server_path(
        &path(1, 8, [0.; 3], [10., 0., 0.]),
        0,
        1.,
        &mut floor,
        &mut output,
    )?;
    owner.input_refresh_pending = false;
    owner.advance_to(250, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((owner.position.x - 2.5).abs() < 0.0001);
    owner.advance_to(1000, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, Vec3::new(10., 0., 0.));
    assert!(output.is_empty());
    Ok(())
}

#[test]
fn releasing_control_stops_the_current_path_without_a_completion_acknowledgment() -> TestResult {
    let (world, mut owner) = alive_world()?;
    let mut input = PlayerInputState::default();
    let mut floor = Floor::new()?;
    let mut output = VecDeque::new();
    owner.receive_server_path(
        &path(1, 19, [0.; 3], [10., 0., 0.]),
        0,
        1.,
        &mut floor,
        &mut output,
    )?;
    owner.refresh_path_input(&mut input, &world, &mut output)?;
    owner.advance_to(250, [0.5, 2., 1.], &mut floor, &mut output)?;
    let stopped = owner.position;
    owner.command(
        MovementCommand::Control(PlayerControlEvent::ActiveMover {
            previous: 1,
            next: 0,
            timestamp_ms: 250,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    assert!(!owner.path_active());
    owner.advance_to(1250, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, stopped);
    assert!(
        matches!(output.back(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::NotActiveMover)
    );
    assert!(
        !output
            .iter()
            .any(|event| matches!(event, PlayerMovementOutput::SplineDone { .. }))
    );
    Ok(())
}
