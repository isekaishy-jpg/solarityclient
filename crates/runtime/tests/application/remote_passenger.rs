//! Remote timeline behavior on a genuine resident, moving transport collision model.

use glam::Vec3;
use solarity_ecs::{WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldTransform};
use solarity_network::{
    ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport, RemoteMovement,
    WorldMovementKind,
};
use solarity_systems::{MovementBspCacheMode, MovementGroundProfile};

use super::RemoteUnit;
use crate::application::game_object_coordinator::transport_tests::collision::Scene;
use crate::application::player_movement::{LocalMovementGeometry, MovementPhase};
use crate::application::{RuntimeMovementGeometry, RuntimeMovementQuery};

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
