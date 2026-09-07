//! Ordered handler dispatch through encrypted packet decoding and ECS admission.

use super::*;
use crate::application::gameplay_session::{GameplayUpdateError, apply_object_updates_with};
use crate::test_network::{TestError, WorldServer};
use glam::Vec3;
use solarity_ecs::{WorldBootstrap, WorldMapId};
use solarity_systems::project_object_fields;

#[test]
fn packet_notifications_compare_live_words_after_all_raw_blocks() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut session) = WorldServer::connect().await?;
            for (blocks, consume_at_flags, expected) in [
                (
                    vec![vec![(9, 0x80), (14, 0x7FFF_0001), (17, 0)]],
                    false,
                    vec![
                        GameObjectNotification::Flags { previous: 0 },
                        GameObjectNotification::Progress,
                        GameObjectNotification::State,
                    ],
                ),
                // The last raw block snapshots every watched range, even untouched words.
                (
                    vec![vec![(9, 0x80), (17, 0)], vec![(14, 0x7FFF_0001)]],
                    false,
                    vec![
                        GameObjectNotification::State,
                        GameObjectNotification::Progress,
                    ],
                ),
                // Earlier handlers mutate the raw progress compared by the next handler.
                (
                    vec![vec![(9, 0x80), (14, 0x7FFF_0001), (17, 0)]],
                    true,
                    vec![
                        GameObjectNotification::Flags { previous: 0 },
                        GameObjectNotification::State,
                    ],
                ),
                // State's always-notify bit applies even when its byte is unchanged.
                (
                    vec![vec![(17, 1)]],
                    false,
                    vec![GameObjectNotification::State],
                ),
                // Low dynamic flags and the other packed bytes are outside these handlers.
                (
                    vec![vec![(14, 0xFFFF_0001), (17, 0x0001_0001)]],
                    false,
                    vec![GameObjectNotification::State],
                ),
            ] {
                let mut world = ActiveWorld::enter(WorldBootstrap::new(
                    WorldMapId::new(0),
                    7,
                    "Local",
                    Vec3::ZERO,
                    0.0,
                ));
                let initial = [(8, 42), (9, 0), (14, 0xFFFF_0000), (17, 1)];
                world.create_object(9, ObjectKind::GameObject, None, initial)?;
                project_object_fields(&mut world, 9, initial)?;
                server
                    .exchange(vec![(0xA9, values_packet(&blocks))], 0)
                    .await?
                    .await??;
                let packet = session.receive_packet().await?;
                let batch = packet.object_updates()?.ok_or("missing values batch")?;
                let final_state = blocks
                    .iter()
                    .flatten()
                    .rfind(|(index, _)| *index == 17)
                    .map_or(1, |(_, value)| *value as u8);
                let mut actual = Vec::new();
                apply_object_updates_with::<GameplayUpdateError>(
                    &mut world,
                    &batch,
                    0,
                    &mut |world, identity, notification| {
                        assert_eq!(Some(identity), world.object_identity(9));
                        assert_eq!(
                            world
                                .game_object_presentation(9)
                                .map(|fields| fields.state()),
                            Some(final_state)
                        );
                        actual.push(notification);
                        if consume_at_flags
                            && matches!(notification, GameObjectNotification::Flags { .. })
                        {
                            world.consume_game_object_sequence_progress(9)?;
                        }
                        Ok(())
                    },
                )?;
                assert_eq!(actual, expected);
            }
            Ok(())
        })
}

fn values_packet(blocks: &[Vec<(u16, u32)>]) -> Vec<u8> {
    let mut packet = (blocks.len() as u32).to_le_bytes().to_vec();
    for fields in blocks {
        packet.extend_from_slice(&[0, 1, 9, 1]); // values, packed GUID, one mask word
        let mask = fields
            .iter()
            .fold(0_u32, |mask, (index, _)| mask | (1 << index));
        packet.extend_from_slice(&mask.to_le_bytes());
        for (_, value) in fields {
            packet.extend_from_slice(&value.to_le_bytes());
        }
    }
    packet
}

/// 4D3FF0 calls 714250 before the next raw block, while field notifications
/// still see the completed packet. An existing GUID gets no new constructor.
#[test]
fn creation_initializes_from_its_own_fields_before_later_raw_updates() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut session) = WorldServer::connect().await?;
            let mut body = 2_u32.to_le_bytes().to_vec();
            body.extend([2, 1, 9, 5]); // create, packed GUID, GameObject
            body.extend(0x242_u16.to_le_bytes());
            for value in [1_f32, 2., 3., 0.] {
                body.extend(value.to_le_bytes());
            }
            body.extend(123_u32.to_le_bytes());
            body.extend(0_u64.to_le_bytes());
            body.push(1);
            body.extend(((1_u32 << 8) | (1 << 14) | (1 << 17)).to_le_bytes());
            for value in [42_u32, 0x1234_0000, 0x0b01] {
                body.extend(value.to_le_bytes());
            }
            body.extend_from_slice(&values_packet(&[vec![(14, 0x5678_0000), (17, 0x0b00)]])[4..]);
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "Local",
                Vec3::ZERO,
                0.,
            ));
            // Replay the create against an existing lifetime as a full refresh.
            for expected in [
                vec![
                    (GameObjectNotification::Initialize, 1, Some(0x1234)),
                    (GameObjectNotification::Progress, 0, Some(0x5678)),
                    (GameObjectNotification::State, 0, Some(0x5678)),
                ],
                vec![
                    (GameObjectNotification::Progress, 0, Some(0x5678)),
                    (GameObjectNotification::State, 0, Some(0x5678)),
                    (GameObjectNotification::Progress, 0, Some(0x5678)),
                    (GameObjectNotification::State, 0, Some(0x5678)),
                ],
            ] {
                server
                    .exchange(vec![(0xa9, body.clone())], 0)
                    .await?
                    .await??;
                let packet = session.receive_packet().await?;
                let batch = packet.object_updates()?.ok_or("missing create batch")?;
                let mut observed = Vec::new();
                apply_object_updates_with::<TestError>(
                    &mut world,
                    &batch,
                    1000,
                    &mut |world, identity, event| {
                        let fields = world
                            .game_object_presentation(identity.guid())
                            .ok_or("missing presentation")?;
                        observed.push((event, fields.state(), fields.sequence_progress()));
                        if event == GameObjectNotification::Initialize {
                            assert_eq!(
                                world
                                    .game_object_movement(9)
                                    .ok_or("missing movement")?
                                    .transport_clock_ms(1000),
                                123
                            );
                            // A constructor can publish runtime geometry before later blocks.
                            world.update_game_object_animated_pose(
                                9,
                                solarity_systems::game_object_transport_pose(
                                    Vec3::new(4., 5., 6.),
                                    0.,
                                    0.,
                                    0.,
                                )?,
                            )?;
                        }
                        Ok(())
                    },
                )?;
                assert_eq!(observed, expected);
                assert_eq!(
                    world
                        .game_object_movement(9)
                        .ok_or("missing retained movement")?
                        .transport_clock_ms(1001),
                    124
                );
            }
            Ok(())
        })
}
