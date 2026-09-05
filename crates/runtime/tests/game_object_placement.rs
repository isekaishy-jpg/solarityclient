//! Encrypted object creation preserves native GameObject placement inputs.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use glam::Vec3;
use solarity_network::WorldObjectUpdate;
use solarity_runtime::RuntimeGameplayCoordinator;
use transfer_world_server::{TestError, WorldServer};

#[test]
fn encrypted_game_object_creates_preserve_rotations_and_sparse_field_ownership()
-> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
                for index in 0..4 {
                    let packed = (index != 3).then_some(0x4000_0100_0018_0000 + u64::from(index));
                    let body = create_body(packed, (index == 0).then_some(7), 100 + index);
                    // Duplicate creates refresh fields while retaining the old placement.
                    // Out-of-range removal followed by creation starts a new owner.
                    let packets = if index == 3 {
                        vec![(0xA9, vec![1, 0, 0, 0, 4, 1, 0, 0, 0, 1, 9]), (0xA9, body)]
                    } else {
                        vec![(0xA9, body)]
                    };
                    server.exchange(packets, 0).await?.await??;
                    loop {
                        gameplay.service()?;
                        let world = gameplay.world().ok_or("missing world")?;
                        if world
                            .game_object_presentation(9)
                            .is_some_and(|view| view.display_id() == 100 + index)
                        {
                            let movement = world
                                .game_object_movement(9)
                                .ok_or("lost GameObject movement")?;
                            let presentation =
                                world.game_object_presentation(9).ok_or("lost fields")?;
                            assert_eq!(presentation.object_type(), 5);
                            assert_eq!(presentation.flags(), 0x20);
                            assert_eq!(presentation.art_kit(), 7);
                            assert_eq!(presentation.animation_progress(), 99);
                            if index != 3 {
                                assert_eq!(movement.packed_rotation(), 0x4000_0100_0018_0000);
                                let parent = movement.transport().ok_or("lost passenger offset")?;
                                assert_eq!(parent.guid, 7);
                                assert_eq!(parent.position, Vec3::new(-1.5, 2.25, 3.5));
                                assert_eq!(parent.orientation, -0.75);
                            } else {
                                assert_eq!(movement.packed_rotation(), 0);
                                assert!(movement.transport().is_none());
                            }
                            assert_eq!(
                                world.object_transform(9).ok_or("lost position")?.position(),
                                Vec3::new(10.0, 20.0, 30.0)
                            );
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                }
                // A sparse flag update must preserve all four GameObject state bytes
                // and the independently owned movement snapshot.
                let mut values = vec![1, 0, 0, 0, 0, 1, 9];
                fields(&mut values, &[(9, 0x80)]);
                server.exchange(vec![(0xA9, values)], 0).await?.await??;
                loop {
                    gameplay.service()?;
                    let world = gameplay.world().ok_or("missing world")?;
                    if let Some(view) = world.game_object_presentation(9)
                        && view.flags() == 0x80
                    {
                        assert_eq!(view.display_id(), 103);
                        assert_eq!(view.bytes_1(), u32::from_le_bytes([1, 5, 7, 99]));
                        assert_eq!(
                            world
                                .game_object_movement(9)
                                .ok_or("sparse fields erased movement")?
                                .packed_rotation(),
                            0
                        );
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                Ok::<(), TestError>(())
            })
            .await?
        })
}

#[test]
fn truncated_packed_quaternion_cannot_consume_the_next_encrypted_packet() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, mut session) = WorldServer::connect().await?;
                let body = create_body(Some(u64::MAX), Some(7), 100);
                let mut packets = (0..body.len())
                    .map(|end| (0xA9, body[..end].to_vec()))
                    .collect::<Vec<_>>();
                packets.push((0xA9, body.clone()));
                let sent = server.exchange(packets, 0).await?;
                for end in 0..body.len() {
                    assert!(
                        session.receive_packet().await?.object_updates().is_err(),
                        "accepted truncation {end}"
                    );
                }
                let packet = session.receive_packet().await?;
                let batch = packet.object_updates()?.ok_or("lost complete packet")?;
                let WorldObjectUpdate::Create { movement, .. } = &batch.updates()[0] else {
                    return Err("wrong operation".into());
                };
                assert_eq!(movement.packed_rotation(), Some(u64::MAX));
                assert_eq!(
                    movement
                        .position_transport()
                        .ok_or("lost wire passenger")?
                        .guid,
                    7
                );
                sent.await??;
                Ok::<(), TestError>(())
            })
            .await?
        })
}

fn create_body(packed: Option<u64>, parent: Option<u8>, display: u32) -> Vec<u8> {
    let mut body = vec![1, 0, 0, 0, 3, 1, 9, 5];
    let flags: u16 =
        (if parent.is_some() { 0x100 } else { 0x40 }) | (if packed.is_some() { 0x200 } else { 0 });
    body.extend_from_slice(&flags.to_le_bytes());
    if let Some(parent) = parent {
        body.extend_from_slice(&[1, parent]);
    }
    for value in [10.0_f32, 20.0, 30.0] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    if parent.is_some() {
        for value in [-1.5_f32, 2.25, 3.5] {
            body.extend_from_slice(&value.to_le_bytes());
        }
    }
    body.extend_from_slice(&0.5_f32.to_le_bytes());
    if parent.is_some() {
        body.extend_from_slice(&(-0.75_f32).to_le_bytes());
    }
    if let Some(packed) = packed {
        body.extend_from_slice(&packed.to_le_bytes());
    }
    fields(
        &mut body,
        &[
            (4, 1.25_f32.to_bits()),
            (8, display),
            (9, 0x20),
            (17, u32::from_le_bytes([1, 5, 7, 99])),
        ],
    );
    body
}

fn fields(body: &mut Vec<u8>, fields: &[(u16, u32)]) {
    body.push(1);
    body.extend_from_slice(
        &fields
            .iter()
            .fold(0_u32, |mask, (index, _)| mask | (1 << index))
            .to_le_bytes(),
    );
    for (_, value) in fields {
        body.extend_from_slice(&value.to_le_bytes());
    }
}
