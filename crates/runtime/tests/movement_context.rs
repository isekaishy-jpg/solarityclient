//! Complete MovementInfo decoding and live ECS projection over encrypted TCP.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use glam::Vec3;
use solarity_ecs::{WorldMovementContext, WorldMovementFall, WorldMovementTransport};
use solarity_network::WorldObjectUpdate;
use solarity_runtime::RuntimeGameplayCoordinator;

use transfer_world_server::{TestError, WorldServer};

/// Each optional section must survive the actual network-to-ECS projection,
/// including a later update that removes all previously present sections.
#[test]
fn encrypted_living_updates_preserve_and_replace_complete_movement_context() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
                for (index, (flags, context)) in corpus().into_iter().enumerate() {
                    let body = living_body(flags, context, index as u32);
                    server.exchange(vec![(0xA9, body)], 0).await?.await??;
                    loop {
                        gameplay.service()?;
                        let world = gameplay
                            .world()
                            .ok_or("movement projection lost the world")?;
                        if let Some(movement) = world.movement_state(8)
                            && movement.context().timestamp_ms == context.timestamp_ms
                        {
                            assert_eq!(movement.flags(), flags);
                            assert_eq!(movement.context(), context);
                            assert_eq!(movement.speeds().values(), SPEEDS);
                            assert_eq!(
                                world.local_player_transform()?.position(),
                                Vec3::new(index as f32, 20.0, 30.0)
                            );
                            assert_eq!(world.local_player_transform()?.orientation(), 0.75);
                            assert_eq!(
                                movement.transport_guid(),
                                context.transport.map(|transport| transport.guid)
                            );
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                }
                Ok::<(), TestError>(())
            })
            .await?
        })
}

/// Every truncation of a fully populated block must fail without consuming
/// bytes from the next encrypted packet. A following valid block still decodes.
#[test]
fn truncated_movement_context_cannot_consume_the_next_encrypted_packet() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, mut session) = WorldServer::connect().await?;
                let (flags, context) = corpus()
                    .into_iter()
                    .find(|(_, context)| {
                        context
                            .transport
                            .is_some_and(|transport| transport.interpolated_time_ms.is_some())
                            && context.pitch_radians.is_some()
                            && context.falling.is_some()
                            && context.spline_elevation.is_some()
                    })
                    .ok_or("missing complete movement fixture")?;
                let body = living_body(flags, context, 1);
                let mut packets: Vec<_> = (0..body.len())
                    .map(|end| (0xA9, body[..end].to_vec()))
                    .collect();
                packets.push((0xA9, body.clone()));
                let sent = server.exchange(packets, 0).await?;
                for end in 0..body.len() {
                    let packet = session.receive_packet().await?;
                    assert!(
                        packet.object_updates().is_err(),
                        "truncation at {end} was accepted"
                    );
                }
                let packet = session.receive_packet().await?;
                let batch = packet
                    .object_updates()?
                    .ok_or("valid movement packet was not decoded")?;
                let WorldObjectUpdate::Create { movement, .. } = &batch.updates()[0] else {
                    return Err("valid movement packet changed update kind".into());
                };
                let decoded = movement.context().ok_or("missing movement context")?;
                assert_eq!(decoded.timestamp_ms, context.timestamp_ms);
                // 0x987E30 stores cos then sin at +0x70/+0x74; 0x987140 copies
                // them to MovementInfo and 0x4F4ED0 writes that same order.
                let falling = decoded.falling.ok_or("missing falling launch")?;
                assert_eq!(falling.direction_cos, 0.8);
                assert_eq!(falling.direction_sin, -0.6);
                assert_eq!(decoded.transport.ok_or("missing transport")?.seat, -1);
                sent.await??;
                Ok::<(), TestError>(())
            })
            .await?
        })
}

const SPEEDS: [f32; 9] = [2.5, 7.0, 4.5, 4.72, 2.5, 7.0, 4.5, 3.0, 2.0];

/// Exercise all independent field-presence conditions from 0x004F4ED0,
/// preserving opaque high flags and signed/wrapping values independently.
fn corpus() -> Vec<(u64, WorldMovementContext)> {
    let mut cases = Vec::new();
    for transport_mode in 0..3 {
        for pitch_flag in [0, 0x20_0000, 0x0200_0000, 0x0020_0000_0000] {
            for falling in [false, true] {
                for elevation in [false, true] {
                    let mut flags = 0x1000_0000_0000_u64 | pitch_flag;
                    let transport = if transport_mode == 0 {
                        None
                    } else {
                        flags |= 0x200;
                        if transport_mode == 2 {
                            flags |= 0x0400_0000_0000;
                        }
                        Some(WorldMovementTransport {
                            guid: 0xF110_0000_0100_002A,
                            position: Vec3::new(-1.5, 2.25, 0.0),
                            orientation: -0.5,
                            time_ms: 0xFFFF_FFF8,
                            seat: -1,
                            interpolated_time_ms: (transport_mode == 2).then_some(0),
                        })
                    };
                    if falling {
                        flags |= 0x1000;
                    }
                    if elevation {
                        flags |= 0x0400_0000;
                    }
                    cases.push((
                        flags,
                        WorldMovementContext {
                            timestamp_ms: 0xFFFF_FFF0_u32.wrapping_add(cases.len() as u32),
                            transport,
                            pitch_radians: (pitch_flag != 0).then_some(-0.25),
                            fall_time_ms: 1234,
                            falling: falling.then_some(WorldMovementFall {
                                vertical_speed: 7.95,
                                direction_cos: 0.8,
                                direction_sin: -0.6,
                                horizontal_speed: 4.5,
                            }),
                            spline_elevation: elevation.then_some(1.25),
                        },
                    ));
                }
            }
        }
    }
    // INTERPOLATED without ON_TRANSPORT and FALLING_FAR without FALLING
    // do not cause extra fields. This also clears a previous full context.
    cases.push((
        0x0400_0000_2000,
        WorldMovementContext {
            timestamp_ms: 123,
            ..WorldMovementContext::default()
        },
    ));
    cases
}

/// Independent raw CREATE_OBJECT2 fixture following native MovementInfo order.
fn living_body(flags: u64, context: WorldMovementContext, position_x: u32) -> Vec<u8> {
    let mut body = 1_u32.to_le_bytes().to_vec();
    body.extend_from_slice(&[3, 1, 8, 4]);
    body.extend_from_slice(&0x21_u16.to_le_bytes());
    body.extend_from_slice(&(flags as u32).to_le_bytes());
    body.extend_from_slice(&((flags >> 32) as u16).to_le_bytes());
    body.extend_from_slice(&context.timestamp_ms.to_le_bytes());
    for value in [position_x as f32, 20.0, 30.0, 0.75] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    if let Some(transport) = context.transport {
        let bytes = transport.guid.to_le_bytes();
        let mask = bytes.iter().enumerate().fold(0_u8, |mask, (bit, value)| {
            mask | (u8::from(*value != 0) << bit)
        });
        body.push(mask);
        body.extend(bytes.into_iter().filter(|value| *value != 0));
        for value in transport
            .position
            .to_array()
            .into_iter()
            .chain([transport.orientation])
        {
            body.extend_from_slice(&value.to_le_bytes());
        }
        body.extend_from_slice(&transport.time_ms.to_le_bytes());
        body.push(transport.seat as u8);
        if let Some(time) = transport.interpolated_time_ms {
            body.extend_from_slice(&time.to_le_bytes());
        }
    }
    if let Some(pitch) = context.pitch_radians {
        body.extend_from_slice(&pitch.to_le_bytes());
    }
    body.extend_from_slice(&context.fall_time_ms.to_le_bytes());
    if let Some(falling) = context.falling {
        for value in [
            falling.vertical_speed,
            falling.direction_cos,
            falling.direction_sin,
            falling.horizontal_speed,
        ] {
            body.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Some(elevation) = context.spline_elevation {
        body.extend_from_slice(&elevation.to_le_bytes());
    }
    for speed in SPEEDS {
        body.extend_from_slice(&speed.to_le_bytes());
    }
    body.push(0); // Empty update-field mask.
    body
}
