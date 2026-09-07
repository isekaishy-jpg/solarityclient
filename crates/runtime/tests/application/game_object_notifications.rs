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
