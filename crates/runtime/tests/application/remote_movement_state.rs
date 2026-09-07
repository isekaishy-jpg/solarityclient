use super::{RemoteMovementInput, RemoteUnit, Scheduled};
use crate::application::player_movement::tests::{Floor, owner};
use solarity_network::{
    ObjectMovementContext, ObjectMovementFall, RemoteMovement, WorldMovementKind,
};
use solarity_systems::MovementGroundProfile;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn command(kind: WorldMovementKind, flags: u64, timestamp_ms: u32, x: f32) -> RemoteMovement {
    RemoteMovement {
        kind,
        guid: 1,
        flags,
        position: [x, 0.0, 0.0],
        orientation: 0.0,
        context: ObjectMovementContext {
            timestamp_ms,
            transport: None,
            pitch_radians: None,
            fall_time_ms: 0,
            falling: None,
            spline_elevation: None,
        },
    }
}

#[test]
fn remote_walk_run_stop_and_wrap_use_contact_simulation() -> TestResult {
    for origin in [0_u32, u32::MAX - 400] {
        for (flags, speed) in [(1_u64, 7.0), (0x101, 2.5)] {
            let (_, motion) = owner()?;
            let (transform, movement) = motion.snapshot();
            let mut remote = RemoteUnit::new(motion.identity, transform, movement, origin);
            assert!(remote.receive(
                command(WorldMovementKind::StartForward, flags, 0, 0.0),
                origin,
                origin
            )?);
            let mut floor = Floor::new()?;
            for elapsed in [250, 500, 750, 1000] {
                remote.advance(
                    origin.wrapping_add(elapsed),
                    [0.5, 2.0, 1.0],
                    MovementGroundProfile::Other,
                    &mut floor,
                    |_| None,
                )?;
            }
            assert!((remote.snapshot().0.position().x - speed).abs() < 0.00001);
            assert!(remote.snapshot().0.position().z.abs() < 0.001);
            assert!(remote.receive(
                command(WorldMovementKind::Stop, 0, 1000, speed),
                origin.wrapping_add(1000),
                origin.wrapping_add(1000)
            )?);
            remote.advance(
                origin.wrapping_add(1250),
                [0.5, 2.0, 1.0],
                MovementGroundProfile::Other,
                &mut floor,
                |_| None,
            )?;
            assert_eq!(remote.snapshot().0.position().x, speed);
        }
    }
    Ok(())
}

#[test]
fn future_commands_wait_for_their_adjusted_clock() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    remote.receive(command(WorldMovementKind::StartForward, 1, 0, 0.0), 0, 0)?;
    assert!(!remote.receive(command(WorldMovementKind::Stop, 0, 1000, 7.0), 200, 200)?);
    assert_eq!(remote.commands.front().ok_or("queued stop")?.time_ms, 1000);
    let mut floor = Floor::new()?;
    for time in [250, 500, 750] {
        remote.advance(
            time,
            [0.5, 2.0, 1.0],
            MovementGroundProfile::Other,
            &mut floor,
            |_| None,
        )?;
        assert_ne!(remote.snapshot().1.flags() & 1, 0);
    }
    remote.advance(
        1000,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert!(remote.commands.is_empty());
    assert_eq!(remote.snapshot().1.flags() & 1, 0);
    assert_eq!(remote.snapshot().0.position().x, 7.0);
    Ok(())
}

#[test]
fn unavailable_geometry_freezes_travel_until_residency_is_ready() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    remote.receive(command(WorldMovementKind::StartForward, 1, 0, 0.0), 0, 0)?;
    let mut floor = Floor::new()?;
    floor.ready = false;
    remote.advance(
        250,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0, transform);
    floor.ready = true;
    remote.advance(
        500,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert!((remote.snapshot().0.position().x - 1.75).abs() < 0.00001);
    Ok(())
}

#[test]
fn queued_heartbeat_retains_launch_and_secondary_flags() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    let mut launch = command(WorldMovementKind::Jump, 0x0008_0000_1001, 0, 0.0);
    launch.context.falling = Some(ObjectMovementFall {
        vertical_speed: -7.0,
        direction_cos: 1.0,
        direction_sin: 0.0,
        horizontal_speed: 7.0,
    });
    remote.receive(launch, 0, 0)?;
    let retained = remote.snapshot().1.context().falling;
    let mut heartbeat = launch;
    heartbeat.kind = WorldMovementKind::Heartbeat;
    heartbeat.flags = 0x1001;
    heartbeat
        .context
        .falling
        .as_mut()
        .ok_or("launch")?
        .vertical_speed = -1.0;
    heartbeat.context.fall_time_ms = 100;
    remote.commands.push_back(Scheduled {
        message: heartbeat,
        time_ms: 100,
    });
    remote.advance(
        100,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut Floor::new()?,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().1.context().falling, retained);
    assert_eq!(remote.snapshot().1.flags() >> 32 & 8, 8);
    Ok(())
}

#[test]
fn path_replacement_flushes_future_snapshots_before_preparing_its_start() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    let path = solarity_network::MonsterMove {
        guid: 1,
        transport: None,
        control_byte: 0,
        start: [0.0, 0.0, 0.0],
        id: 17,
        facing_type: 0,
        facing: solarity_network::MovementSplineFacing::Direction,
        path: Some(solarity_network::MonsterMovePath {
            flags: 0,
            duration_ms: 5000,
            animation: None,
            parabolic: None,
            points: vec![[10.0, 0.0, 0.0]],
        }),
    };
    let mut floor = Floor::new()?;
    remote.process(
        [
            RemoteMovementInput::Baseline {
                transform,
                movement,
                spline: None,
                receipt_ms: 0,
            },
            RemoteMovementInput::Command {
                message: command(WorldMovementKind::StartForward, 1, 0, 0.0),
                receipt_ms: 0,
            },
            RemoteMovementInput::Command {
                message: command(WorldMovementKind::Stop, 0, 1000, 7.0),
                receipt_ms: 200,
            },
            RemoteMovementInput::Path {
                message: path,
                receipt_ms: 200,
                stop_distance_tolerance: 1.0,
            },
        ],
        200,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert!(remote.commands.is_empty());
    assert_eq!(remote.snapshot().0.position().x, 7.0);
    assert_eq!(remote.path.as_ref().ok_or("path")?.id(), 17);
    remote.advance(
        450,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert!(remote.snapshot().0.position().x > 7.0);
    assert!(remote.snapshot().0.position().x < 7.3);
    // An ordinary correction releases the path, so the next frame cannot
    // publish the old spline's position over the new snapshot.
    remote.receive(command(WorldMovementKind::Stop, 0, 1250, 8.0), 450, 450)?;
    assert!(remote.path.is_none());
    remote.advance(
        700,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut floor,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0.position().x, 8.0);
    Ok(())
}

#[test]
fn root_discards_queued_translation_but_keeps_facing_corrections() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement.with_flags(0x800), 0);
    remote.commands.push_back(Scheduled {
        message: command(WorldMovementKind::StartForward, 1, 10, 20.0),
        time_ms: 10,
    });
    remote.advance(
        10,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut Floor::new()?,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0, transform);
    assert_eq!(remote.snapshot().1.flags() & 0x801, 0x800);
    let mut turn = command(WorldMovementKind::SetFacing, 0x800, 20, 0.0);
    turn.orientation = 1.0;
    remote.commands.push_back(Scheduled {
        message: turn,
        time_ms: 20,
    });
    remote.advance(
        20,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut Floor::new()?,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0.orientation(), 1.0);
    Ok(())
}

#[test]
fn heartbeat_ending_a_fall_emits_the_native_landing_notification() -> TestResult {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    let mut jump = command(WorldMovementKind::Jump, 0x1001, 0, 0.0);
    jump.position[2] = 5.0;
    jump.context.falling = Some(ObjectMovementFall {
        vertical_speed: -7.0,
        direction_cos: 1.0,
        direction_sin: 0.0,
        horizontal_speed: 7.0,
    });
    remote.receive(jump, 0, 0)?;
    remote.receive(command(WorldMovementKind::Heartbeat, 0, 10, 0.0), 10, 10)?;
    remote.advance(
        10,
        [0.5, 2.0, 1.0],
        MovementGroundProfile::Other,
        &mut Floor::new()?,
        |_| None,
    )?;
    assert!(
        remote
            .animation_events
            .iter()
            .chain(
                remote
                    .motion
                    .iter()
                    .flat_map(|motion| motion.animation_events.iter())
            )
            .any(|event| matches!(
                event.kind,
                crate::application::unit_animation::UnitMovementAnimationEventKind::Land { .. }
            ))
    );
    Ok(())
}

#[test]
fn idle_units_do_not_probe_ground_and_outside_map_travel_is_not_integrated() -> TestResult {
    let (_, motion) = owner()?;
    let (_, movement) = motion.snapshot();
    for (position, flags) in [
        (glam::Vec3::new(0.0, 0.0, 12.0), 0),
        (glam::Vec3::new(17065.0, 0.0, 0.0), 1),
    ] {
        let transform = solarity_ecs::WorldTransform::new(position, 0.0);
        let mut remote = RemoteUnit::new(motion.identity, transform, movement.with_flags(flags), 0);
        remote.advance(
            250,
            [0.5, 2.0, 1.0],
            MovementGroundProfile::Other,
            &mut Floor::new()?,
            |_| None,
        )?;
        assert_eq!(remote.snapshot().0, transform);
        assert_eq!(remote.snapshot().1.flags() & 0x1000, 0);
    }
    Ok(())
}
