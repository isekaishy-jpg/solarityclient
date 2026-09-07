//! Native monster path preparation through the public protocol/systems boundary.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use glam::Vec3;
use solarity_ecs::WorldTransform;
use solarity_network::MonsterMove;
use solarity_systems::{MovementPathRequest, PreparedMovementPath};

use transfer_world_server::{TestError, WorldServer};

/// A new server path starts an idle unit, continues from its advanced position
/// on replacement, and stops at the server point without retaining stale motion.
#[test]
fn encrypted_monster_paths_start_replace_stop_and_respect_guid_lifetimes() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, session) = WorldServer::connect().await?;
            let mut gameplay = solarity_runtime::GameplaySession::enter(session);
            let speeds = solarity_ecs::WorldMovementSpeeds::new([
                2.5,
                7.0,
                4.5,
                4.72,
                2.5,
                7.0,
                4.5,
                std::f32::consts::PI,
                std::f32::consts::PI,
            ]);
            gameplay.world_mut().create_object(
                9,
                solarity_ecs::ObjectKind::Unit,
                Some(WorldTransform::new(Vec3::ZERO, 0.0)),
                [],
            )?;
            gameplay.world_mut().update_movement(
                9,
                solarity_ecs::WorldMovementState::new(
                    0,
                    speeds,
                    solarity_ecs::WorldMovementContext::default(),
                ),
            )?;
            let sent = server
                .exchange(
                    vec![
                        (0xdd, linear_message(9, 10.0, 5000)),
                        (0xdd, linear_message(9, 10.0, 5000)),
                        (0xdd, stop_message(9, 2.5)),
                        (0xdd, linear_message(9, 20.0, 5000)),
                    ],
                    0,
                )
                .await?;
            let packet = gameplay.network_mut().receive_packet().await?;
            assert!(gameplay.apply_monster_move_at(
                &packet.monster_move()?.ok_or("lost path")?,
                1000,
                1.0
            )?);
            let movement = gameplay.world().movement_state(9).ok_or("lost movement")?;
            assert_eq!(
                solarity_systems::resolve_unit_locomotion_animation(movement).animation_id(),
                4
            );
            solarity_systems::advance_world_movement_splines(gameplay.world(), 2000)?;
            assert_eq!(
                gameplay
                    .world()
                    .object_transform(9)
                    .ok_or("lost unit")?
                    .position(),
                Vec3::new(2.0, 0.0, 0.0)
            );
            let packet = gameplay.network_mut().receive_packet().await?;
            assert!(gameplay.apply_monster_move_at(
                &packet.monster_move()?.ok_or("lost replacement")?,
                2250,
                1.0
            )?);
            solarity_systems::advance_world_movement_splines(gameplay.world(), 2250)?;
            assert_eq!(
                gameplay
                    .world()
                    .object_transform(9)
                    .ok_or("lost replacement")?
                    .position(),
                Vec3::new(2.0, 0.0, 0.0)
            );
            let packet = gameplay.network_mut().receive_packet().await?;
            assert!(gameplay.apply_monster_move_at(
                &packet.monster_move()?.ok_or("lost stop")?,
                2500,
                1.0
            )?);
            solarity_systems::advance_world_movement_splines(gameplay.world(), 9000)?;
            assert_eq!(
                gameplay
                    .world()
                    .object_transform(9)
                    .ok_or("lost stopped unit")?
                    .position(),
                Vec3::new(2.5, 0.0, 0.0)
            );
            let stopped = gameplay
                .world()
                .movement_state(9)
                .ok_or("lost stopped movement")?;
            assert_eq!(solarity_systems::resolve_unit_movement_speed(stopped), 0.0);
            gameplay.world_mut().remove_object(9)?;
            let packet = gameplay.network_mut().receive_packet().await?;
            assert!(!gameplay.apply_monster_move_at(
                &packet.monster_move()?.ok_or("lost late path")?,
                10000,
                1.0
            )?);
            assert!(gameplay.world().entity_by_guid(9).is_none());
            sent.await??;
            Ok(())
        })
}

/// Exact short form from 0073C8E0, with no flags, duration, or point-count tail.
fn stop_message(guid: u8, x: f32) -> Vec<u8> {
    let mut body = vec![1, guid, 0];
    for value in [x, 0.0, 0.0] {
        body.extend(value.to_le_bytes());
    }
    body.extend(37_u32.to_le_bytes());
    body.push(1);
    body
}

/// One linear destination needs no packed intermediate points.
fn linear_message(guid: u8, x: f32, duration: u32) -> Vec<u8> {
    let mut body = stop_message(guid, 0.0);
    body.pop();
    body.push(0);
    for word in [0_u32, duration, 1] {
        body.extend(word.to_le_bytes());
    }
    for value in [x, 0.0, 0.0] {
        body.extend(value.to_le_bytes());
    }
    body
}

#[test]
fn decoded_commands_match_original_path_controls_and_duration() -> Result<(), TestError> {
    let mut count = 0;
    for (index, line) in include_str!("../../systems/tests/fixtures/monster-path-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split(" | ").collect();
        let inputs = words(parts[0])?;
        let mut body = vec![1, 9, 0];
        body.extend(bytes(parts[1])?);
        let message = MonsterMove::decode(0xdd, &body)?.ok_or("lost monster move")?;
        let mut expected = parts[2].splitn(2, ' ');
        let kind = expected.next().ok_or("lost native kind")?;
        let values = words(expected.next().ok_or("lost native result")?)?;
        assert_eq!(message.id, values[2]);
        let request = match message.path {
            Some(path) => MovementPathRequest::Move {
                start: Vec3::from_array(message.start),
                points: path.points.into_iter().map(Vec3::from_array).collect(),
                flags: path.flags,
                duration_ms: path.duration_ms,
            },
            None => MovementPathRequest::Stop {
                destination: Vec3::from_array(message.start),
            },
        };
        let current = WorldTransform::new(
            Vec3::new(
                f32::from_bits(inputs[0]),
                f32::from_bits(inputs[1]),
                f32::from_bits(inputs[2]),
            ),
            f32::from_bits(inputs[3]),
        );
        let prepared = request.prepare(current, f32::from_bits(inputs[4]), 4.0)?;
        let actual = match prepared {
            PreparedMovementPath::Spline {
                controls,
                flags,
                duration_ms,
                ..
            } => {
                assert_eq!(kind, "path", "row {index}");
                assert_eq!(duration_ms, values[0], "row {index}");
                assert_eq!(flags, values[1], "row {index}");
                controls
            }
            PreparedMovementPath::Place { destination, flags } => {
                assert_eq!(kind, "place", "row {index}");
                assert_eq!(flags, values[1], "row {index}");
                vec![destination]
            }
        };
        assert_eq!(actual.len() * 3, values.len() - 3, "row {index}");
        for (component, (actual, expected)) in actual
            .iter()
            .flat_map(|point| point.to_array())
            .zip(&values[3..])
            .enumerate()
        {
            let expected = f32::from_bits(*expected);
            assert!(
                (actual - expected).abs() <= 0.00001_f32.max(expected.abs() * 0.000001),
                "row {index} component {component}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 540);
    Ok(())
}

/// Every native conditional tail is bounded before a packet can be applied.
#[test]
fn transport_facing_and_effect_fields_survive_and_truncations_fail() -> Result<(), TestError> {
    let mut body = vec![1, 9, 1, 7, 0xff, 1];
    for value in [1.0_f32, 2.0, 3.0] {
        body.extend(value.to_le_bytes());
    }
    body.extend(37_u32.to_le_bytes());
    body.push(3);
    body.extend(0x0102030405060708_u64.to_le_bytes());
    body.extend(0x00200800_u32.to_le_bytes());
    body.push(2);
    body.extend(250_u32.to_le_bytes());
    body.extend(2000_u32.to_le_bytes());
    body.extend(4.5_f32.to_le_bytes());
    body.extend(500_u32.to_le_bytes());
    body.extend(1_u32.to_le_bytes());
    for value in [4.0_f32, 5.0, 6.0] {
        body.extend(value.to_le_bytes());
    }
    for end in 0..body.len() {
        assert!(
            MonsterMove::decode(0x2ae, &body[..end]).is_err(),
            "prefix {end}"
        );
    }
    let message = MonsterMove::decode(0x2ae, &body)?.ok_or("missing message")?;
    let parent = message.transport.ok_or("missing transport")?;
    assert_eq!((parent.guid, parent.seat), (7, -1));
    assert_eq!(message.control_byte, 1);
    assert_eq!(
        message.facing,
        solarity_network::MovementSplineFacing::Target(0x0102030405060708)
    );
    let path = message.path.ok_or("missing path")?;
    assert_eq!(path.animation, Some((2, 250)));
    assert_eq!(path.parabolic, Some((4.5, 500)));
    assert_eq!(path.points, [[4.0, 5.0, 6.0]]);
    body.push(0);
    assert!(MonsterMove::decode(0x2ae, &body).is_err());
    Ok(())
}

fn words(text: &str) -> Result<Vec<u32>, TestError> {
    text.split_whitespace()
        .map(|word| Ok(u32::from_str_radix(word, 16)?))
        .collect()
}

fn bytes(text: &str) -> Result<Vec<u8>, TestError> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
