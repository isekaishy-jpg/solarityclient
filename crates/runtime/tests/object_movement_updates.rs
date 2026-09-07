//! Native opcode-one movement layout through encrypted framing and ECS admission.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use glam::Vec3;
use solarity_ecs::{GameObjectMovement, ObjectKind, WorldTransform};
use solarity_network::{MovementSplineFacing, ObjectMovementUpdate, WorldObjectUpdate};
use solarity_runtime::{GameplaySession, GameplayUpdateError};
use transfer_world_server::{TestError, WorldServer};

const SPEEDS: [f32; 9] = [2.5, 7.0, 4.5, 4.75, 2.5, 7.25, 4.75, 3.125, 3.25];
const CONDITIONAL_FLAGS: u64 = 0x0420_0E20_1200;

/// The transport clock survives encrypted create decoding and wraps with the
/// local receipt clock; a repeated create must not re-anchor an existing boat.
#[test]
fn game_object_transport_clock_is_retained_across_receipt_and_guid_refresh() -> Result<(), TestError>
{
    run(async {
        let (server, session) = WorldServer::connect().await?;
        let mut gameplay = GameplaySession::enter(session);
        let sent = server
            .exchange(
                vec![
                    (0xA9, transport_create_body(9, 0)),
                    (0xA9, transport_create_body(10, u32::MAX - 5)),
                    (0xA9, transport_create_body(9, 999)),
                ],
                0,
            )
            .await?;
        for (guid, progress, receipt, expected_later) in
            [(9, 0, u32::MAX - 15, 30), (10, u32::MAX - 5, 100, 24)]
        {
            let packet = gameplay.network_mut().receive_packet().await?;
            let batch = packet.object_updates()?.ok_or("missing transport create")?;
            let WorldObjectUpdate::Create { movement, .. } = &batch.updates()[0] else {
                return Err("wrong transport operation".into());
            };
            assert_eq!(movement.transport_progress_ms(), Some(progress));
            gameplay.apply_object_updates_at(&batch, receipt)?;
            let movement = gameplay
                .world()
                .game_object_movement(guid)
                .ok_or("lost transport clock")?;
            assert_eq!(movement.transport_clock_ms(receipt), progress);
            assert_eq!(
                movement.transport_clock_ms(receipt.wrapping_add(30)),
                expected_later
            );
        }
        let original = gameplay
            .world()
            .game_object_movement(9)
            .ok_or("lost original boat")?;
        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates_at(
            &packet.object_updates()?.ok_or("missing repeated create")?,
            1000,
        )?;
        assert_eq!(gameplay.world().game_object_movement(9), Some(original));
        sent.await??;
        Ok(())
    })
}

/// Minimal native create layout with the independent transport clock and rotation.
fn transport_create_body(guid: u8, progress_ms: u32) -> Vec<u8> {
    let mut body = 1_u32.to_le_bytes().to_vec();
    body.extend_from_slice(&[2, 1, guid, 5]);
    body.extend_from_slice(&0x242_u16.to_le_bytes());
    floats(&mut body, &[1., 2., 3., 0.]);
    body.extend_from_slice(&progress_ms.to_le_bytes());
    body.extend_from_slice(&0_u64.to_le_bytes());
    body.push(1);
    body.extend_from_slice(&((1_u32 << 3) | (1 << 8) | (1 << 17)).to_le_bytes());
    for value in [1001_u32, 33, 0x0f00] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body
}

/// Encrypted movement updates retain the server clock and drive both ECS
/// position and the stock walk/run speed threshold between received packets.
#[test]
fn retained_remote_path_walks_stops_and_is_removed_by_a_new_snapshot() -> Result<(), TestError> {
    run(async {
        let (server, session) = WorldServer::connect().await?;
        let mut gameplay = GameplaySession::enter(session);
        gameplay.world_mut().create_object(
            9,
            ObjectKind::Unit,
            Some(WorldTransform::new(Vec3::ZERO, 0.0)),
            [],
        )?;
        let sent = server
            .exchange(
                vec![
                    (0xA9, movement_body(9, 0x0800_0001, 77)),
                    (0xA9, movement_body(9, 0x0800_0001, 78)),
                    (0xA9, movement_body(9, 0, 79)),
                ],
                0,
            )
            .await?;
        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates_at(&packet.object_updates()?.ok_or("missing path")?, 1000)?;
        let movement = gameplay.world().movement_state(9).ok_or("lost movement")?;
        assert_eq!(
            solarity_systems::resolve_unit_locomotion_animation(movement).animation_id(),
            4
        );
        assert!(
            (solarity_systems::resolve_unit_movement_speed(movement) - 3.0_f32.sqrt() / 2.0).abs()
                < 0.000001
        );
        solarity_systems::advance_world_movement_splines(gameplay.world(), 1125)?;
        assert_eq!(
            gameplay
                .world()
                .object_transform(9)
                .ok_or("lost unit")?
                .position(),
            Vec3::new(10.125, 21.125, 32.125)
        );
        solarity_systems::advance_world_movement_splines(gameplay.world(), 2875)?;
        let completed = gameplay.world().object_transform(9).ok_or("lost unit")?;
        assert_eq!(
            completed,
            WorldTransform::new(Vec3::new(12.0, 23.0, 34.0), 0.75)
        );
        let stopped = gameplay
            .world()
            .movement_state(9)
            .ok_or("lost stopped movement")?;
        assert_eq!(
            solarity_systems::resolve_unit_locomotion_animation(stopped).animation_id(),
            0
        );
        assert_eq!(solarity_systems::resolve_unit_movement_speed(stopped), 0.0);

        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates_at(
            &packet.object_updates()?.ok_or("missing replacement")?,
            3000,
        )?;
        solarity_systems::advance_world_movement_splines(gameplay.world(), 3125)?;
        assert_eq!(
            gameplay
                .world()
                .object_transform(9)
                .ok_or("lost replacement")?
                .position(),
            Vec3::new(10.125, 21.125, 32.125)
        );
        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates_at(&packet.object_updates()?.ok_or("missing stop")?, 3200)?;
        solarity_systems::advance_world_movement_splines(gameplay.world(), 5000)?;
        assert_eq!(
            gameplay
                .world()
                .object_transform(9)
                .ok_or("lost stop")?
                .position(),
            Vec3::new(10.0, 20.0, 30.0)
        );
        assert!(
            gameplay
                .world()
                .movement_state(9)
                .ok_or("lost movement")?
                .spline()
                .is_none()
        );
        sent.await??;
        Ok(())
    })
}

#[test]
fn movement_decoder_matches_original_native_reader_snapshots() -> Result<(), TestError> {
    run(async {
        let (server, mut session) = WorldServer::connect().await?;
        for line in include_str!("fixtures/object-movement-native.txt").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (wire, native) = line.split_once(" | ").ok_or("invalid native fixture")?;
            let wire = hex_bytes(wire)?;
            let native = hex_bytes(native)?;
            let mut body = vec![1, 0, 0, 0, 1, 1, 9];
            body.extend_from_slice(&wire);
            server.exchange(vec![(0xA9, body)], 0).await?.await??;
            let packet = session.receive_packet().await?;
            let batch = packet.object_updates()?.ok_or("missing native fixture")?;
            let WorldObjectUpdate::Movement { movement, .. } = &batch.updates()[0] else {
                return Err("wrong native fixture operation".into());
            };
            let flags = movement.movement_flags().ok_or("lost flags")?;
            assert_eq!(flags as u32, word(&native, 0x10)?);
            // Native consumes INTERPOLATED into its second clock. The wire
            // boundary deliberately retains both the original bit and presence.
            assert_eq!((flags >> 32) as u16 & !0x400, word(&native, 0x14)? as u16);
            let context = movement.context().ok_or("lost native context")?;
            assert_eq!(context.timestamp_ms, word(&native, 0)?);
            for (index, value) in movement
                .position()
                .ok_or("lost position")?
                .iter()
                .enumerate()
            {
                assert_eq!(value.to_bits(), word(&native, 0x28 + index * 4)?);
            }
            assert_eq!(
                movement.orientation().ok_or("lost facing")?.to_bits(),
                word(&native, 0x34)?
            );
            assert_eq!(context.fall_time_ms, word(&native, 0x3C)?);
            if let Some(transport) = context.transport {
                assert_eq!(
                    transport.guid,
                    u64::from(word(&native, 8)?) | (u64::from(word(&native, 12)?) << 32)
                );
                assert_eq!(transport.seat, native[0x16] as i8);
                for (index, value) in transport.position.iter().enumerate() {
                    assert_eq!(value.to_bits(), word(&native, 0x18 + index * 4)?);
                }
                assert_eq!(transport.orientation.to_bits(), word(&native, 0x24)?);
                assert_eq!(transport.time_ms, word(&native, 0x54)?);
                assert_eq!(
                    transport.interpolated_time_ms.unwrap_or(transport.time_ms),
                    word(&native, 0x58)?
                );
            }
            if let Some(pitch) = context.pitch_radians {
                assert_eq!(pitch.to_bits(), word(&native, 0x38)?);
            }
            if let Some(fall) = context.falling {
                for (index, value) in [
                    fall.vertical_speed,
                    fall.direction_cos,
                    fall.direction_sin,
                    fall.horizontal_speed,
                ]
                .iter()
                .enumerate()
                {
                    assert_eq!(value.to_bits(), word(&native, 0x40 + index * 4)?);
                }
            }
            if let Some(elevation) = context.spline_elevation {
                assert_eq!(elevation.to_bits(), word(&native, 0x50)?);
            }
            for (index, value) in movement
                .speeds()
                .ok_or("lost native speeds")?
                .values()
                .iter()
                .enumerate()
            {
                assert_eq!(value.to_bits(), word(&native, 0x60 + index * 4)?);
            }
        }
        Ok(())
    })
}

#[test]
fn movement_only_updates_share_living_fields_without_creation_prefixes() -> Result<(), TestError> {
    run(async {
        let (server, mut session) = WorldServer::connect().await?;
        // Each body follows the native 0x004D6DA0 -> 0x004F5090 call path.
        // A following VALUES operation detects any over/under-consumption.
        for flags in [0, 1, 0x0200, CONDITIONAL_FLAGS] {
            let mut body = 2_u32.to_le_bytes().to_vec();
            body.extend_from_slice(&[1, 1, 9]);
            body.extend(living(flags, 123));
            body.extend_from_slice(&[0, 1, 9, 1]);
            body.extend_from_slice(&(1_u32 << 3).to_le_bytes());
            body.extend_from_slice(&42_u32.to_le_bytes());
            server.exchange(vec![(0xA9, body)], 0).await?.await??;
            let packet = session.receive_packet().await?;
            let batch = packet.object_updates()?.ok_or("missing update")?;
            let WorldObjectUpdate::Movement { guid, movement } = &batch.updates()[0] else {
                return Err("wrong movement operation".into());
            };
            assert_eq!(*guid, 9);
            assert_living(movement, flags, 123)?;
            assert_eq!(movement.update_flags(), 0);
            assert!(!movement.is_self());
            assert!(movement.position_transport().is_none());
            assert!(movement.packed_rotation().is_none());
            let WorldObjectUpdate::Values { fields, .. } = &batch.updates()[1] else {
                return Err("lost following fields".into());
            };
            assert_eq!((fields[0].index(), fields[0].value()), (3, 42));

            // CREATE still prefixes kind + UpdateFlag and then reads the same
            // living snapshot, followed by its own field-mask count.
            let mut create = vec![1, 0, 0, 0, 3, 1, 9, 3, 0x20, 0];
            create.extend(living(flags, 123));
            create.push(0);
            server.exchange(vec![(0xA9, create)], 0).await?.await??;
            let packet = session.receive_packet().await?;
            let batch = packet.object_updates()?.ok_or("missing create")?;
            let WorldObjectUpdate::Create {
                movement, fields, ..
            } = &batch.updates()[0]
            else {
                return Err("wrong create operation".into());
            };
            assert_living(movement, flags, 123)?;
            assert_eq!(movement.update_flags(), 0x20);
            assert!(fields.is_empty());
        }
        Ok(())
    })
}

#[test]
fn movement_only_updates_preserve_packet_order_and_local_player_control() -> Result<(), TestError> {
    run(async {
        let (server, session) = WorldServer::connect().await?;
        let mut gameplay = GameplaySession::enter(session);
        let original_local = gameplay.world().object_transform(8);
        let mut body = vec![3, 0, 0, 0, 3, 1, 9, 3, 0x20, 0];
        body.extend(living(0, 1));
        body.push(0);
        body.extend_from_slice(&[1, 1, 8]);
        body.extend(living(CONDITIONAL_FLAGS, 200));
        body.extend_from_slice(&[1, 1, 9]);
        body.extend(living(CONDITIONAL_FLAGS, 201));
        server.exchange(vec![(0xA9, body)], 0).await?.await??;
        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates(&packet.object_updates()?.ok_or("missing batch")?)?;
        assert_eq!(gameplay.world().object_transform(8), original_local);
        assert!(gameplay.world().movement_state(8).is_none());
        let remote = gameplay
            .world()
            .movement_state(9)
            .ok_or("missing remote movement")?;
        assert_eq!(remote.context().timestamp_ms, 201);
        assert_eq!(remote.speeds().values(), SPEEDS);
        assert_eq!(remote.context().transport.ok_or("lost attachment")?.guid, 7);
        assert_eq!(
            remote.context().falling.ok_or("lost fall")?.vertical_speed,
            8.5
        );
        assert_eq!(
            gameplay
                .world()
                .object_transform(9)
                .ok_or("lost position")?
                .position(),
            Vec3::new(10.0, 20.0, 30.0)
        );

        // A subsequent remote block without conditional flags detaches and
        // clears the previous fall/pitch context instead of retaining it.
        server
            .exchange(vec![(0xA9, movement_body(9, 0, 202))], 0)
            .await?
            .await??;
        let packet = gameplay.network_mut().receive_packet().await?;
        gameplay.apply_object_updates(&packet.object_updates()?.ok_or("missing detach")?)?;
        let context = gameplay
            .world()
            .movement_state(9)
            .ok_or("lost movement")?
            .context();
        assert_eq!(context.timestamp_ms, 202);
        assert!(context.transport.is_none());
        assert!(context.falling.is_none());
        assert!(context.pitch_radians.is_none());
        Ok(())
    })
}

#[test]
fn living_updates_cannot_erase_game_object_placement() -> Result<(), TestError> {
    run(async {
        let (server, session) = WorldServer::connect().await?;
        let mut gameplay = GameplaySession::enter(session);
        let transform = WorldTransform::new(Vec3::new(4.0, 5.0, 6.0), 0.75);
        gameplay
            .world_mut()
            .create_object(9, ObjectKind::GameObject, Some(transform), [])?;
        gameplay
            .world_mut()
            .update_game_object_movement(9, GameObjectMovement::new(0x4000_0000_0000_0000, None))?;
        server
            .exchange(vec![(0xA9, movement_body(9, 0, 1))], 0)
            .await?
            .await??;
        let packet = gameplay.network_mut().receive_packet().await?;
        let error = gameplay
            .apply_object_updates(&packet.object_updates()?.ok_or("missing batch")?)
            .err()
            .ok_or("accepted non-living movement")?;
        assert_eq!(
            error,
            GameplayUpdateError::NonLivingMovement {
                guid: 9,
                kind: ObjectKind::GameObject
            }
        );
        assert_eq!(gameplay.world().object_transform(9), Some(transform));
        assert_eq!(
            gameplay
                .world()
                .game_object_movement(9)
                .ok_or("lost rotation")?
                .packed_rotation(),
            0x4000_0000_0000_0000
        );
        assert!(gameplay.world().movement_state(9).is_none());
        Ok(())
    })
}

#[test]
fn truncated_movement_blocks_do_not_consume_the_next_encrypted_frame() -> Result<(), TestError> {
    run(async {
        let (server, mut session) = WorldServer::connect().await?;
        let body = movement_body(9, CONDITIONAL_FLAGS, 123);
        let mut packets = (0..body.len())
            .map(|end| (0xA9, body[..end].to_vec()))
            .collect::<Vec<_>>();
        let mut wrong_prefix = movement_body(9, 0, 123);
        wrong_prefix.splice(7..7, [0x20, 0]);
        packets.push((0xA9, wrong_prefix));
        packets.push((0xA9, body.clone()));
        let sent = server.exchange(packets, 0).await?;
        for end in 0..=body.len() {
            assert!(
                session.receive_packet().await?.object_updates().is_err(),
                "accepted malformed packet {end}"
            );
        }
        let packet = session.receive_packet().await?;
        let batch = packet.object_updates()?.ok_or("lost final frame")?;
        let WorldObjectUpdate::Movement { movement, .. } = &batch.updates()[0] else {
            return Err("wrong final operation".into());
        };
        assert_living(movement, CONDITIONAL_FLAGS, 123)?;
        sent.await??;
        Ok(())
    })
}

fn assert_living(
    movement: &ObjectMovementUpdate,
    flags: u64,
    timestamp: u32,
) -> Result<(), TestError> {
    assert_eq!(movement.movement_flags(), Some(flags));
    assert_eq!(movement.position(), Some([10.0, 20.0, 30.0]));
    assert_eq!(movement.orientation(), Some(0.5));
    assert_eq!(movement.speeds().ok_or("missing speeds")?.values(), SPEEDS);
    if flags & 0x0800_0000 != 0 {
        let spline = movement.spline().ok_or("lost spline snapshot")?;
        assert_eq!(spline.flags, 0x20000);
        assert_eq!(spline.facing, MovementSplineFacing::Angle(0.75));
        assert_eq!(
            (spline.elapsed_ms, spline.duration_ms, spline.id),
            (125, 2000, 37)
        );
        assert_eq!(spline.timing_parameters, [1.0, 1.25, 2.5]);
        assert_eq!(spline.effect_start_ms, 250);
        assert_eq!(
            spline.nodes,
            [
                [9.0, 20.0, 31.0],
                [10.0, 21.0, 32.0],
                [11.0, 22.0, 33.0],
                [12.0, 23.0, 34.0]
            ]
        );
        assert_eq!(spline.mode, 0);
        assert_eq!(spline.destination, [12.0, 23.0, 34.0]);
    } else {
        assert!(movement.spline().is_none());
    }
    let context = movement.context().ok_or("missing context")?;
    assert_eq!(context.timestamp_ms, timestamp);
    assert_eq!(context.fall_time_ms, 75);
    if flags & 0x200 != 0 {
        let transport = context.transport.ok_or("lost transport")?;
        assert_eq!(transport.guid, 7);
        assert_eq!(transport.position, [-1.5, 2.25, 3.5]);
        assert_eq!(transport.orientation, -0.75);
        assert_eq!(transport.time_ms, 25);
        assert_eq!(transport.seat, -1);
        assert_eq!(
            transport.interpolated_time_ms,
            (flags & 0x0400_0000_0000 != 0).then_some(26)
        );
    } else {
        assert!(context.transport.is_none());
    }
    if flags == CONDITIONAL_FLAGS {
        assert_eq!(context.pitch_radians, Some(0.25));
        let fall = context.falling.ok_or("lost fall")?;
        assert_eq!(
            [
                fall.vertical_speed,
                fall.direction_cos,
                fall.direction_sin,
                fall.horizontal_speed
            ],
            [8.5, 0.6, 0.8, 7.0]
        );
        assert_eq!(context.spline_elevation, Some(1.25));
    }
    Ok(())
}

fn movement_body(guid: u8, flags: u64, timestamp: u32) -> Vec<u8> {
    let mut body = vec![1, 0, 0, 0, 1, 1, guid];
    body.extend(living(flags, timestamp));
    body
}

// Field order recovered from 0x004F4D40/0x004F5090. This fixture deliberately
// owns its bytes independently of the project's movement encoders.
fn living(flags: u64, timestamp: u32) -> Vec<u8> {
    let mut body = flags.to_le_bytes()[..6].to_vec();
    body.extend_from_slice(&timestamp.to_le_bytes());
    floats(&mut body, &[10.0, 20.0, 30.0, 0.5]);
    if flags & 0x200 != 0 {
        body.extend_from_slice(&[1, 7]);
        floats(&mut body, &[-1.5, 2.25, 3.5, -0.75]);
        body.extend_from_slice(&25_u32.to_le_bytes());
        body.push(0xFF);
        if flags & 0x0400_0000_0000 != 0 {
            body.extend_from_slice(&26_u32.to_le_bytes());
        }
    }
    if flags & 0x0020_0220_0000 != 0 {
        floats(&mut body, &[0.25]);
    }
    body.extend_from_slice(&75_u32.to_le_bytes());
    if flags & 0x1000 != 0 {
        floats(&mut body, &[8.5, 0.6, 0.8, 7.0]);
    }
    if flags & 0x0400_0000 != 0 {
        floats(&mut body, &[1.25]);
    }
    floats(&mut body, &SPEEDS);
    if flags & 0x0800_0000 != 0 {
        body.extend_from_slice(&0x20000_u32.to_le_bytes());
        floats(&mut body, &[0.75]);
        for word in [125_u32, 2000, 37] {
            body.extend_from_slice(&word.to_le_bytes());
        }
        floats(&mut body, &[1.0, 1.25, 2.5]);
        body.extend_from_slice(&250_u32.to_le_bytes());
        body.extend_from_slice(&4_u32.to_le_bytes());
        floats(
            &mut body,
            &[
                9.0, 20.0, 31.0, 10.0, 21.0, 32.0, 11.0, 22.0, 33.0, 12.0, 23.0, 34.0,
            ],
        );
        body.push(0);
        floats(&mut body, &[12.0, 23.0, 34.0]);
    }
    body
}

fn floats(body: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        body.extend_from_slice(&value.to_le_bytes());
    }
}

fn hex_bytes(value: &str) -> Result<Vec<u8>, TestError> {
    (0..value.len())
        .step_by(2)
        .map(|offset| {
            Ok(u8::from_str_radix(
                value
                    .get(offset..offset + 2)
                    .ok_or("truncated fixture hex")?,
                16,
            )?)
        })
        .collect()
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, TestError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("truncated native snapshot")?
            .try_into()?,
    ))
}

fn run(future: impl std::future::Future<Output = Result<(), TestError>>) -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async { tokio::time::timeout(std::time::Duration::from_secs(10), future).await? })
}
