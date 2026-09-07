//! Local movement and frozen wire snapshots through a real resident transport deck.

use std::collections::VecDeque;

use glam::Vec3;
use solarity_ecs::{WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldTransform};
use solarity_network::{WorldMovementKind, WorldMovementMessage};
use solarity_systems::MovementBspCacheMode;

use super::super::{
    LocalMovement, LocalMovementGeometry, MovementCommand, PlayerMovementOutput,
    RuntimePlayerMovement,
};
use crate::application::game_object_coordinator::transport_tests::collision::Scene;
use crate::application::{RuntimeMovementGeometry, RuntimeMovementQuery};
use crate::test_network::{TestError, WorldServer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn world_replacement_and_authoritative_detach_suppress_old_parent_notifications() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut owner = mover(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        assert!(owner.contact_passenger(9, &mut geometry)?);
    }
    let (transform, movement) = owner.snapshot();
    owner.published = (transform, movement);
    scene
        .world
        .update_local_movement(owner.identity, transform, movement)?;
    let identity = owner.identity;
    let mut runtime = RuntimePlayerMovement {
        owner: Some(owner),
        ..RuntimePlayerMovement::default()
    };
    scene.world.remove_object(9)?;
    runtime.retire_passenger(None, &scene.objects, 100)?;
    let replacement = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(1),
        7,
        "Replacement",
        Vec3::ZERO,
        0.,
    ));
    runtime.retire_passenger(Some(&replacement), &scene.objects, 100)?;
    let mut context = movement.context();
    context.transport = None;
    let detached = WorldMovementState::new(movement.flags() & !0x200, movement.speeds(), context);
    scene
        .world
        .update_local_movement(identity, transform, detached)?;
    runtime.retire_passenger(Some(&scene.world), &scene.objects, 100)?;
    assert!(runtime.output.is_empty());
    assert!(runtime.commands.is_empty());
    scene.objects.synchronize(Some(&scene.world))?;
    let geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x8010_8111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    let admitted = LocalMovement::new_in_geometry(identity, transform, detached, 100, &geometry)?
        .ok_or("unparented correction")?;
    assert_eq!(admitted.world_position(), transform.position());
    assert!(admitted.passenger.is_none());
    Ok(())
}

#[test]
fn transport_retirement_detaches_before_matrix_release_and_rechecks_support() -> TestResult {
    for corrected in [false, true] {
        let mut scene = Scene::passenger_deck()?;
        let mut owner = mover(&scene)?;
        let mut query = RuntimeMovementQuery::new();
        {
            let mut geometry = RuntimeMovementGeometry::new(
                &mut scene.terrain,
                &scene.world,
                &scene.objects,
                0x8010_8111,
                MovementBspCacheMode::Enabled,
                &mut query,
            );
            assert!(owner.contact_passenger(9, &mut geometry)?);
        }
        let parent = owner.passenger.ok_or("parent")?;
        let frame = parent.frame;
        let (transform, movement) = owner.snapshot();
        owner.published = (transform, movement);
        scene
            .world
            .update_local_movement(owner.identity, transform, movement)?;
        let mut runtime = RuntimePlayerMovement {
            owner: Some(owner),
            ..RuntimePlayerMovement::default()
        };
        runtime.retire_passenger(Some(&scene.world), &scene.objects, 90)?;
        assert!(
            runtime.output.is_empty(),
            "a resident parent is not retiring"
        );
        let expected = if corrected {
            let mut context = movement.context();
            context
                .transport
                .as_mut()
                .ok_or("correction parent")?
                .position = Vec3::new(0.25, -0.1, 0.8);
            let expected =
                frame.world_position(context.transport.ok_or("correction parent")?.position);
            scene.world.update_local_movement(
                runtime.owner.as_ref().ok_or("mover")?.identity,
                WorldTransform::new(expected, transform.orientation()),
                WorldMovementState::new(movement.flags(), movement.speeds(), context),
            )?;
            expected
        } else {
            transform.position()
        };
        scene.world.remove_object(9)?;
        runtime.retire_passenger(Some(&scene.world), &scene.objects, 100)?;
        let owner = runtime.owner.as_ref().ok_or("mover")?;
        assert_eq!(owner.world_position(), expected);
        assert!(owner.passenger.is_none());
        let leave = decode(packet(&runtime.output, WorldMovementKind::ChangeTransport)?)?;
        assert_eq!(leave.position, expected.to_array());
        assert_eq!(leave.context.timestamp_ms, 100);
        assert_eq!(leave.context.transport.ok_or("destruction leave")?.guid, 0);
        assert_eq!(
            leave.context.transport.ok_or("destruction leave")?.time_ms,
            3362
        );
        let recheck = runtime.commands.pop_front().ok_or("support recheck")?;
        assert!(matches!(
            recheck,
            MovementCommand::SupportRecheck { timestamp_ms: 100 }
        ));
        scene.objects.synchronize(Some(&scene.world))?;
        assert!(
            scene
                .objects
                .object_movement_frame(parent.identity)?
                .is_none()
        );
        runtime.retire_passenger(Some(&scene.world), &scene.objects, 110)?;
        assert!(runtime.commands.is_empty(), "destruction is delivered once");
        let owner = runtime.owner.as_mut().ok_or("mover")?;
        owner.time_ms = 100;
        owner.command(
            recheck,
            &mut runtime.input,
            &scene.world,
            &mut runtime.output,
        )?;
        assert!(owner.snapshot().1.context().falling.is_some());
        let recheck_packet = decode(packet(&runtime.output, WorldMovementKind::Heartbeat)?)?;
        assert!(recheck_packet.context.transport.is_none());
        assert_eq!(recheck_packet.position, expected.to_array());
    }
    Ok(())
}

/// Captures the sole writer's encrypted packet, then decodes its MovementInfo
/// through the server reader. ChangeTransport shares the heartbeat field layout.
fn decode(
    message: WorldMovementMessage,
) -> Result<solarity_network::RemoteMovement, Box<dyn std::error::Error>> {
    let result: Result<_, TestError> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let (mut reader, mut writer) = session.split();
                let capture = server.exchange_raw(Vec::new(), 1).await?;
                writer.send_movement(&message).await?;
                let mut packets = capture.await??;
                let (opcode, body) = packets.pop().ok_or("missing captured packet")?;
                assert_eq!(opcode, message.kind() as u32);
                server.exchange_raw(vec![(0xee, body)], 0).await?.await??;
                reader
                    .receive_packet()
                    .await?
                    .remote_movement()?
                    .ok_or("missing decoded movement".into())
            })
            .await?
        });
    result.map_err(|error| error as Box<dyn std::error::Error>)
}

/// Uses a real world identity and the ordinary player dimensions and speed bank.
fn mover(scene: &Scene) -> Result<LocalMovement, Box<dyn std::error::Error>> {
    Ok(LocalMovement::new(
        scene.world.object_identity(7).ok_or("player identity")?,
        WorldTransform::new(Vec3::new(20., 5., 0.05), 0.),
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
    )?)
}

/// Preserves non-movement output while selecting the notification under test.
fn packet(
    output: &VecDeque<PlayerMovementOutput>,
    kind: WorldMovementKind,
) -> Result<WorldMovementMessage, Box<dyn std::error::Error>> {
    output
        .iter()
        .find_map(|output| match output {
            PlayerMovementOutput::Movement(message) if message.kind() == kind => Some(*message),
            _ => None,
        })
        .ok_or("missing movement notification".into())
}

#[test]
fn landing_boards_real_deck_idle_follows_parent_and_packets_keep_local_coordinates() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut owner = mover(&scene)?;
    let mut output = VecDeque::new();
    let mut query = RuntimeMovementQuery::new();
    owner.acquire_mover(&mut output)?;
    output.clear();
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        owner.refresh_passenger(&mut geometry)?;
        owner.advance_to(100, [0.25, 1.5, 0.5], &mut geometry, &mut output)?;
    }
    let landed = decode(packet(&output, WorldMovementKind::FallLand)?)?;
    let transport = landed.context.transport.ok_or("landing transport")?;
    assert_eq!(transport.guid, 9);
    assert_eq!(transport.time_ms, 3362);
    assert_eq!(transport.seat, -1);
    assert_eq!(transport.interpolated_time_ms, None);
    assert!(transport.position.iter().all(|value| value.abs() < 0.001));
    assert!(output.iter().all(|output| !matches!(output, PlayerMovementOutput::Movement(message) if message.kind() == WorldMovementKind::ChangeTransport)));
    assert_eq!(
        owner.flags & 0x200,
        0,
        "wire transport bit must not block input admission"
    );
    assert_eq!(owner.snapshot().1.flags() & 0x200, 0x200);
    let local = owner.position;
    let original = owner.world_position();
    scene.synchronize(6100)?;
    output.clear();
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        owner.refresh_passenger(&mut geometry)?;
        owner.advance_to(200, [0.25, 1.5, 0.5], &mut geometry, &mut output)?;
    }
    assert_eq!(owner.position, local);
    assert_ne!(owner.world_position(), original);
    owner.emit(WorldMovementKind::Heartbeat, &mut output)?;
    let riding = decode(packet(&output, WorldMovementKind::Heartbeat)?)?;
    assert_eq!(
        riding.context.transport.ok_or("riding transport")?.position,
        local.to_array()
    );
    assert_eq!(
        riding.context.transport.ok_or("riding transport")?.time_ms,
        6000
    );
    assert_eq!(riding.position, owner.world_position().to_array());
    // A later scene must not rebuild a queued packet under writer backpressure.
    let frozen = packet(&output, WorldMovementKind::Heartbeat)?;
    scene.synchronize(6200)?;
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        owner.refresh_passenger(&mut geometry)?;
        owner.flags |= 1;
        owner.reanchor()?;
        owner.advance_to(220, [0.25, 1.5, 0.5], &mut geometry, &mut output)?;
    }
    assert!(
        owner.position.distance(local) > 0.1,
        "walking in parent coordinates: {:?} -> {:?}",
        local,
        owner.position
    );
    assert_eq!(packet(&output, WorldMovementKind::Heartbeat)?, frozen);
    Ok(())
}

#[test]
fn retention_rejected_contact_and_guid_zero_leave_follow_native_packet_rules() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut owner = mover(&scene)?;
    let mut output = VecDeque::new();
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x8010_8111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    assert!(owner.contact_passenger(9, &mut geometry)?);
    assert!(
        !owner.contact_passenger(0, &mut geometry)?,
        "inside native collision bounds retains the parent"
    );
    assert!(
        !owner.contact_passenger(123456, &mut geometry)?,
        "rejected nonzero candidate must not detach"
    );
    owner.position.x = 2.;
    let expected = owner.world_position();
    assert!(owner.contact_passenger(0, &mut geometry)?);
    assert_eq!(owner.position, expected);
    assert!(owner.snapshot().1.context().transport.is_none());
    owner.emit(WorldMovementKind::ChangeTransport, &mut output)?;
    let leave = decode(packet(&output, WorldMovementKind::ChangeTransport)?)?;
    let transport = leave.context.transport.ok_or("explicit leave block")?;
    assert_eq!(transport.guid, 0);
    assert_eq!(transport.position, expected.to_array());
    assert_eq!(transport.time_ms, 3362);
    assert_eq!(transport.seat, -1);
    assert_eq!(leave.flags & 0x200, 0x200);
    owner.emit(WorldMovementKind::Heartbeat, &mut output)?;
    let unparented = decode(packet(&output, WorldMovementKind::Heartbeat)?)?;
    assert!(unparented.context.transport.is_none());
    assert_eq!(unparented.flags & 0x200, 0);
    Ok(())
}

#[test]
fn remote_contact_does_not_board_and_authoritative_local_coordinates_survive_admission()
-> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut owner = mover(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x8010_8111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    owner.remote = true;
    assert!(!owner.contact_passenger(9, &mut geometry)?);
    owner.remote = false;
    assert!(owner.contact_passenger(9, &mut geometry)?);
    let (transform, movement) = owner.snapshot();
    let admitted =
        LocalMovement::new_in_geometry(owner.identity, transform, movement, 200, &geometry)?
            .ok_or("admitted parent")?;
    assert_eq!(admitted.position, owner.position);
    assert_eq!(admitted.orientation, owner.orientation);
    assert_eq!(admitted.world_position(), transform.position());
    assert_eq!(admitted.snapshot().1.transport_guid(), Some(9));
    assert!(LocalMovementGeometry::passenger(&geometry, 999)?.is_none());
    owner.passenger_seat = 3;
    assert!(
        !owner.contact_passenger(9, &mut geometry)?,
        "seat-only contact does not change parent coordinates"
    );
    assert_eq!(owner.passenger_seat, -1);
    owner.camera = crate::application::player_camera::PlayerCameraInput::new(
        solarity_ecs::PlayerViewState::default(),
        0.7,
    );
    let mut output = VecDeque::new();
    owner.set_mouse_facing(&mut crate::input::PlayerInputState::default(), &mut output)?;
    assert!((owner.world_orientation() - 0.7).abs() < 0.000001);
    let facing = decode(packet(&output, WorldMovementKind::SetFacing)?)?;
    assert!((facing.orientation - 0.7).abs() < 0.000001);
    assert_eq!(
        facing
            .context
            .transport
            .ok_or("facing transport")?
            .orientation,
        owner.orientation
    );
    Ok(())
}

#[test]
fn switching_parents_publishes_new_phase_and_consumes_second_clock_once() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    scene.add_passenger_deck(11)?;
    let mut owner = mover(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut output = VecDeque::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x8010_8111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    assert!(owner.contact_passenger(9, &mut geometry)?);
    owner.emit(WorldMovementKind::ChangeTransport, &mut output)?;
    let first = decode(packet(&output, WorldMovementKind::ChangeTransport)?)?;
    assert_eq!(
        first
            .context
            .transport
            .ok_or("first boarding")?
            .interpolated_time_ms,
        None
    );
    output.clear();
    assert!(owner.contact_passenger(11, &mut geometry)?);
    let transport = owner
        .snapshot()
        .1
        .context()
        .transport
        .ok_or("second parent")?;
    assert_eq!(transport.time_ms, 3862);
    assert_eq!(transport.interpolated_time_ms, Some(3362));
    assert_eq!(
        owner.snapshot().1.context().transport,
        Some(transport),
        "presentation cannot consume a wire clock"
    );
    owner.emit(WorldMovementKind::ChangeTransport, &mut output)?;
    let switched = decode(packet(&output, WorldMovementKind::ChangeTransport)?)?;
    assert_eq!(switched.flags & (0x400_u64 << 32), 0x400_u64 << 32);
    assert_eq!(
        switched
            .context
            .transport
            .ok_or("switched block")?
            .interpolated_time_ms,
        Some(3362)
    );
    owner.emit(WorldMovementKind::Heartbeat, &mut output)?;
    let subsequent = decode(packet(&output, WorldMovementKind::Heartbeat)?)?;
    assert_eq!(subsequent.flags & (0x400_u64 << 32), 0);
    assert_eq!(
        subsequent
            .context
            .transport
            .ok_or("subsequent block")?
            .time_ms,
        3862
    );
    assert_eq!(
        subsequent
            .context
            .transport
            .ok_or("subsequent block")?
            .interpolated_time_ms,
        None
    );
    output.clear();
    assert!(owner.contact_passenger(9, &mut geometry)?);
    owner.position.x = 2.;
    assert!(owner.contact_passenger(0, &mut geometry)?);
    owner.emit(WorldMovementKind::ChangeTransport, &mut output)?;
    let leave = decode(packet(&output, WorldMovementKind::ChangeTransport)?)?;
    let leave = leave.context.transport.ok_or("leave block")?;
    assert_eq!(
        (leave.guid, leave.time_ms, leave.interpolated_time_ms),
        (0, 3362, Some(3862))
    );
    Ok(())
}

#[test]
fn airborne_volume_exit_rebases_before_motion_and_preserves_fall_time() -> TestResult {
    let mut scene = Scene::passenger_deck()?;
    let mut owner = mover(&scene)?;
    let mut query = RuntimeMovementQuery::new();
    let mut output = VecDeque::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x8010_8111,
        MovementBspCacheMode::Enabled,
        &mut query,
    );
    assert!(owner.contact_passenger(9, &mut geometry)?);
    owner.position.x = 2.;
    owner.acquire_mover(&mut output)?;
    output.clear();
    let expected = owner.world_position();
    owner.advance_to(100, [0.25, 1.5, 0.5], &mut geometry, &mut output)?;
    assert_eq!(
        owner.position, expected,
        "7618B0's pre-motion leave consumes zero collision time"
    );
    assert_eq!(owner.snapshot().1.context().fall_time_ms, 0);
    assert!(owner.snapshot().1.context().falling.is_some());
    let leave = decode(packet(&output, WorldMovementKind::ChangeTransport)?)?;
    assert_eq!(leave.context.timestamp_ms, 0);
    assert_eq!(leave.context.transport.ok_or("leave block")?.guid, 0);
    assert!(
        output
            .iter()
            .all(|output| !matches!(output, PlayerMovementOutput::SkippedTime { .. }))
    );
    owner.advance_to(200, [0.25, 1.5, 0.5], &mut geometry, &mut output)?;
    assert!(owner.position.z < expected.z);
    assert_eq!(owner.snapshot().1.context().fall_time_ms, 100);
    Ok(())
}
