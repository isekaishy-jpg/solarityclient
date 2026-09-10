//! Remote timeline behavior on a genuine resident, moving transport collision model.

use glam::Vec3;
use solarity_ecs::{WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldTransform};
use solarity_network::{
    MonsterMove, MonsterMovePath, MonsterMoveTransport, MovementSplineFacing,
    ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport, RemoteMovement,
    WorldMovementKind,
};
use solarity_systems::{MovementBspCacheMode, MovementGroundProfile};

use super::RemoteUnit;
use crate::application::game_object_coordinator::transport_tests::collision::Scene;
use crate::application::player_movement::{LocalMovementGeometry, MovementPhase};
use crate::application::{RuntimeMovementGeometry, RuntimeMovementQuery};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn passenger_notifications_follow_execution_and_distinguish_flush_from_playback() -> TestResult {
    use crate::application::unit_animation::UnitMovementAnimationEventKind;

    for flush in [false, true] {
        let mut scene = Scene::passenger_deck()?;
        let mut remote = owner(&scene)?;
        let mut query = RuntimeMovementQuery::new();
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        assert!(remote.receive(
            command(WorldMovementKind::StartForward, 1, 0, -0.3),
            0,
            0,
            &mut geometry
        )?);
        let first = remote
            .animation_events
            .pop_front()
            .ok_or("initial passenger event")?;
        assert!(matches!(
            first.kind,
            UnitMovementAnimationEventKind::Passenger {
                previous: None,
                parent: Some(_),
                animated: false,
                ..
            }
        ));
        remote.animation_events.clear();
        let mut changed_seat = command(WorldMovementKind::Stop, 0, 100, 0.4);
        changed_seat
            .context
            .transport
            .as_mut()
            .ok_or("transport")?
            .seat = 2;
        assert!(!remote.receive(changed_seat, 10, 10, &mut geometry)?);
        assert!(
            remote.animation_events.is_empty(),
            "receipt cannot start the transition"
        );
        if flush {
            remote.flush(&mut geometry)?;
        } else {
            remote.advance(
                100,
                [0.1, 1.5, 0.5],
                MovementGroundProfile::Other,
                &mut geometry,
                |_| None,
            )?;
        }
        let events = remote
            .animation_events
            .iter()
            .filter_map(|event| {
                if let UnitMovementAnimationEventKind::Passenger {
                    previous,
                    parent,
                    animated,
                    ..
                } = event.kind
                {
                    Some((previous, parent, animated, event.movement))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, Some((9, -1)), "seat-only changes reach F60");
        assert_eq!(events[0].1, scene.world.object_identity(9));
        assert_eq!(events[0].2, !flush);
        assert_eq!(
            events[0].3.context().transport.ok_or("new transport")?.seat,
            2
        );
        remote.animation_events.clear();
        remote.receive_path(&path(None), 110, 1., &mut geometry, |_| None)?;
        let exit = remote
            .animation_events
            .iter()
            .find(|event| matches!(event.kind, UnitMovementAnimationEventKind::Passenger { .. }))
            .ok_or("path exit")?;
        assert!(
            matches!(
                exit.kind,
                UnitMovementAnimationEventKind::Passenger {
                    previous: Some((9, 2)),
                    parent: None,
                    animated: true,
                    ..
                }
            ),
            "MonsterMove sets A30 bit 20000000 around parent admission"
        );
    }
    Ok(())
}

/// Use independent wire world/local poses so a dropped transport block is visible.
fn command(kind: WorldMovementKind, flags: u64, time_ms: u32, local_x: f32) -> RemoteMovement {
    RemoteMovement {
        kind,
        guid: 7,
        flags: flags | 0x200 | (0x400_u64 << 32),
        position: [900., 901., 902.],
        orientation: 2.5,
        context: ObjectMovementContext {
            timestamp_ms: time_ms,
            transport: Some(ObjectMovementTransport {
                guid: 9,
                position: [local_x, -0.5, 0.],
                orientation: 0.,
                time_ms: 8888,
                seat: -1,
                interpolated_time_ms: Some(7777),
            }),
            pitch_radians: None,
            fall_time_ms: 0,
            falling: None,
            spline_elevation: None,
        },
    }
}

/// A short linear server path whose points and final-facing data are parent-local.
fn path(parent: Option<u64>) -> MonsterMove {
    MonsterMove {
        guid: 7,
        transport: parent.map(|guid| MonsterMoveTransport { guid, seat: -1 }),
        control_byte: 0,
        start: [-0.3, -0.5, 0.],
        id: 42,
        facing_type: 3,
        facing: MovementSplineFacing::Target(33),
        path: Some(MonsterMovePath {
            flags: 0,
            duration_ms: 100,
            animation: None,
            parabolic: None,
            points: vec![[0.4, -0.5, 0.]],
        }),
    }
}

#[test]
fn transport_path_moves_locally_resolves_world_targets_and_follows_after_completion() -> TestResult
{
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        remote.receive(
            command(WorldMovementKind::Heartbeat, 0, 0, -0.3),
            0,
            0,
            &mut geometry,
        )?;
        remote.receive_path(&path(Some(9)), 0, 1., &mut geometry, |_| None)?;
        remote.advance(
            50,
            [0.1, 1.5, 0.5],
            MovementGroundProfile::Other,
            &mut geometry,
            |_| None,
        )?;
    }
    let transport = remote
        .snapshot()
        .1
        .context()
        .transport
        .ok_or("path transport")?;
    assert!(
        (transport.position.x - 0.05).abs() < 0.00001,
        "{transport:?}"
    );
    assert_eq!(transport.position.y, -0.5);
    scene.synchronize(6100)?;
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        let frame = geometry.passenger(9)?.ok_or("parent frame")?.frame;
        let target = frame.world_position(Vec3::new(0.4, 0.5, 0.));
        remote.advance(
            100,
            [0.1, 1.5, 0.5],
            MovementGroundProfile::Other,
            &mut geometry,
            |guid| (guid == 33).then_some(target),
        )?;
        let (world, movement) = remote.snapshot();
        let transport = movement.context().transport.ok_or("completed transport")?;
        assert_eq!(transport.position, Vec3::new(0.4, -0.5, 0.));
        assert!(
            (transport.orientation - std::f32::consts::FRAC_PI_2).abs() < 0.00001,
            "{transport:?}"
        );
        assert_eq!(world.position(), frame.world_position(transport.position));
        assert_eq!(
            world.orientation(),
            frame.world_orientation(transport.orientation)
        );
        assert_eq!(movement.flags() & 3, 0);
        assert_ne!(movement.spline().ok_or("completed path")?.flags & 0x400, 0);
    }
    let completed = remote.snapshot();
    scene.synchronize(6500)?;
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        remote.advance(
            100,
            [0.1, 1.5, 0.5],
            MovementGroundProfile::Other,
            &mut geometry,
            |_| None,
        )?;
    }
    assert_ne!(remote.snapshot().0.position(), completed.0.position());
    assert_eq!(
        remote.snapshot().1.context().transport,
        completed.1.context().transport
    );
    Ok(())
}

#[test]
fn path_parent_rejection_preserves_attachment_and_world_path_replacement_detaches() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x0010_0111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    remote.receive(
        command(WorldMovementKind::StartForward, 1, 0, -0.3),
        0,
        0,
        &mut geometry,
    )?;
    let before = remote.snapshot();
    remote.receive_path(&path(Some(999)), 0, 1., &mut geometry, |_| None)?;
    assert!(remote.path.is_none());
    assert_eq!(remote.snapshot().0, before.0);
    assert_eq!(remote.snapshot().1.transport_guid(), Some(9));
    assert_eq!(
        remote.snapshot().1.flags() & 3,
        0,
        "flush stops ordinary axes even when parent admission fails"
    );
    remote.receive_path(&path(Some(9)), 0, 1., &mut geometry, |_| None)?;
    remote.advance(
        50,
        [0.1, 1.5, 0.5],
        MovementGroundProfile::Other,
        &mut geometry,
        |_| None,
    )?;
    let before = remote.snapshot();
    remote.receive_path(&path(Some(999)), 50, 1., &mut geometry, |_| None)?;
    assert_eq!(remote.path.as_ref().ok_or("retained path")?.id(), 42);
    assert_eq!(remote.snapshot(), before);
    let mut replacement = path(None);
    replacement.start = before.0.position().to_array();
    replacement.path.as_mut().ok_or("replacement")?.points =
        vec![(before.0.position() + Vec3::X).to_array()];
    replacement.id = 43;
    remote.receive_path(&replacement, 50, 1., &mut geometry, |_| None)?;
    assert_eq!(remote.snapshot().0.position(), before.0.position());
    assert!(remote.snapshot().1.transport_guid().is_none());
    remote.advance(
        100,
        [0.1, 1.5, 0.5],
        MovementGroundProfile::Other,
        &mut geometry,
        |_| None,
    )?;
    assert!((remote.snapshot().0.position().x - before.0.position().x - 0.5).abs() < 0.00001);
    Ok(())
}

/// The scene supplies real identity, terrain, route, and passenger matrices.
fn owner(scene: &Scene) -> Result<RemoteUnit, Box<dyn std::error::Error>> {
    Ok(RemoteUnit::new(
        scene.world.object_identity(7).ok_or("unit identity")?,
        WorldTransform::new(Vec3::new(20., 5., 0.), 0.),
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
        0,
    ))
}

#[test]
fn idle_remote_follows_parent_at_zero_elapsed_and_keeps_received_clock_metadata() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let first;
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        assert!(remote.receive(
            command(WorldMovementKind::Heartbeat, 0, 0, 0.1),
            0,
            0,
            &mut geometry
        )?);
        first = remote.snapshot().0;
        assert_ne!(first.position(), Vec3::new(900., 901., 902.));
    }
    scene.synchronize(6100)?;
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x0010_0111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        remote.advance(
            0,
            [0.25, 1.5, 0.5],
            MovementGroundProfile::Other,
            &mut geometry,
            |_| None,
        )?;
    }
    let (transform, movement) = remote.snapshot();
    let transport = movement.context().transport.ok_or("remote transport")?;
    assert_ne!(first.position(), transform.position());
    assert_eq!(transport.position, Vec3::new(0.1, -0.5, 0.));
    assert_eq!(transport.time_ms, 8888);
    assert_eq!(transport.interpolated_time_ms, Some(7777));
    let motion = remote.motion.as_ref().ok_or("remote motion")?;
    let frame = motion.passenger.ok_or("passenger frame")?.frame;
    assert_eq!(
        transform.position(),
        frame.world_position(transport.position)
    );
    assert_eq!(transform.orientation(), frame.world_orientation(0.));
    assert_eq!(motion.flags & 0x200, 0);
    assert_eq!(movement.flags() & 0x200, 0x200);
    Ok(())
}

#[test]
fn queued_remote_stop_blends_and_executes_in_passenger_coordinates() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x0010_0111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    assert!(remote.receive(
        command(WorldMovementKind::StartForward, 1, 0, -0.3),
        0,
        0,
        &mut geometry
    )?);
    assert!(!remote.receive(
        command(WorldMovementKind::Stop, 0, 100, 0.4),
        10,
        10,
        &mut geometry
    )?);
    assert!(remote.motion.as_ref().ok_or("motion")?.blend.is_some());
    remote.advance(
        50,
        [0.1, 1.5, 0.5],
        MovementGroundProfile::Other,
        &mut geometry,
        |_| None,
    )?;
    let midway = remote
        .snapshot()
        .1
        .context()
        .transport
        .ok_or("midpoint transport")?
        .position;
    assert!(midway.x > -0.3 && midway.x < 0.4, "{midway:?}");
    remote.advance(
        100,
        [0.1, 1.5, 0.5],
        MovementGroundProfile::Other,
        &mut geometry,
        |_| None,
    )?;
    let (transform, movement) = remote.snapshot();
    assert!(remote.commands.is_empty());
    assert_eq!(
        movement.flags() & 1,
        0,
        "wire transport bit must not block Stop"
    );
    let transport = movement.context().transport.ok_or("stopped transport")?;
    assert_eq!(transport.position, Vec3::new(0.4, -0.5, 0.));
    assert_eq!(
        transform.position(),
        remote
            .motion
            .as_ref()
            .ok_or("motion")?
            .passenger
            .ok_or("frame")?
            .frame
            .world_position(transport.position)
    );
    Ok(())
}

#[test]
fn immediate_missing_parent_uses_wire_world_pose_and_clears_attachment() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x0010_0111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    remote.receive(
        command(WorldMovementKind::Heartbeat, 0, 0, 0.),
        0,
        0,
        &mut geometry,
    )?;
    let mut missing = command(WorldMovementKind::Heartbeat, 0, 100, 0.2);
    missing
        .context
        .transport
        .as_mut()
        .ok_or("wire parent")?
        .guid = 999;
    assert!(remote.receive(missing, 100, 100, &mut geometry)?);
    let (transform, movement) = remote.snapshot();
    assert_eq!(transform.position(), Vec3::from_array(missing.position));
    assert_eq!(transform.orientation(), missing.orientation);
    assert!(movement.context().transport.is_none());
    assert_eq!(movement.flags() & (0x200 | (0x400_u64 << 32)), 0);
    Ok(())
}

#[test]
fn queued_parent_switch_preserves_and_rotates_the_airborne_launch_basis() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    scene.add_passenger_deck(10)?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x0010_0111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    let mut launch = command(WorldMovementKind::Jump, 0x1001, 0, 0.);
    launch.context.falling = Some(ObjectMovementFall {
        vertical_speed: -7.,
        direction_cos: 1.,
        direction_sin: 0.,
        horizontal_speed: 7.,
    });
    assert!(remote.receive(launch, 0, 0, &mut geometry)?);
    let old = geometry.passenger(9)?.ok_or("old frame")?.frame;
    let next = geometry.passenger(10)?.ok_or("new frame")?.frame;
    let expected = next
        .entry_change()
        .direction(old.exit_change().direction(Vec3::X));
    let mut switched = command(WorldMovementKind::Heartbeat, 0x1001, 100, 0.4);
    switched.context.transport.as_mut().ok_or("transport")?.guid = 10;
    switched.context.fall_time_ms = 100;
    switched.context.falling = Some(ObjectMovementFall {
        vertical_speed: 55.,
        direction_cos: 0.,
        direction_sin: -1.,
        horizontal_speed: 99.,
    });
    assert!(!remote.receive(switched, 10, 10, &mut geometry)?);
    assert!(
        remote.motion.as_ref().ok_or("motion")?.blend.is_none(),
        "different parent spaces must not blend"
    );
    remote.flush(&mut geometry)?;
    let motion = remote.motion.as_ref().ok_or("motion")?;
    let MovementPhase::Fall(fall) = motion.phase else {
        return Err("airborne owner".into());
    };
    let fall = fall.snapshot();
    assert_eq!(fall.direction, expected);
    assert_eq!(fall.initial_downward_speed, -7.);
    assert_eq!(fall.horizontal_speed, 7.);
    assert_eq!(fall.fall_time_ms, 100);
    assert_eq!(remote.snapshot().1.transport_guid(), Some(10));
    assert_eq!(motion.position, Vec3::new(0.4, -0.5, 0.));
    assert_eq!(
        remote.snapshot().0.position(),
        next.world_position(motion.position)
    );
    Ok(())
}

#[test]
fn rejected_queued_parent_unlinks_before_leaving_the_old_local_pose_untouched() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut remote = owner(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x0010_0111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    remote.receive(
        command(WorldMovementKind::StartForward, 1, 0, -0.3),
        0,
        0,
        &mut geometry,
    )?;
    let mut missing = command(WorldMovementKind::Stop, 0, 100, 0.4);
    missing.context.transport.as_mut().ok_or("transport")?.guid = 999;
    assert!(!remote.receive(missing, 10, 10, &mut geometry)?);
    remote.flush(&mut geometry)?;
    let (transform, movement) = remote.snapshot();
    assert_eq!(transform.position(), Vec3::new(-0.3, -0.5, 0.));
    assert_eq!(
        movement.flags() & 1,
        1,
        "rejected snapshot does not replace movement flags"
    );
    assert!(movement.transport_guid().is_none());
    Ok(())
}
