//! Server world-state changes pass through setup, live dispatch, and map transfer.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use solarity_asset::{AreaSoundReferences, WorldStateZoneSound};
use solarity_media::{ZoneSoundLocationIds, resolve_world_state_zone_sounds};
use solarity_network::WorldTransfer;
use solarity_runtime::RuntimeGameplayCoordinator;
use transfer_world_server::{TestError, WorldServer, location_body};

#[test]
fn world_state_audio_conditions_follow_packets_and_survive_map_replacement() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, mut session) = WorldServer::connect().await?;
                server
                    .exchange(
                        vec![(0x2c2, initial([0, 12, 0], &[(77, 1), (88, 9), (77, 2)]))],
                        0,
                    )
                    .await?
                    .await??;
                let setup = session.receive_packet().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), session, vec![setup])?;
                let location = ZoneSoundLocationIds {
                    areas: [12, 0],
                    ..Default::default()
                };
                let sounds = AreaSoundReferences {
                    zone_music_id: 7,
                    ..Default::default()
                };
                let rows = [WorldStateZoneSound {
                    state: [77, 2],
                    area_id: 12,
                    world_model_area_id: 0,
                    sounds,
                }];
                let state = gameplay
                    .world()
                    .ok_or("missing world")?
                    .world_state_values();
                assert_eq!(
                    resolve_world_state_zone_sounds(&rows, location, |key| state.value(key)),
                    Some(sounds)
                );
                server
                    .exchange(
                        vec![
                            (
                                0x2c3,
                                [77u32, 0].into_iter().flat_map(u32::to_le_bytes).collect(),
                            ),
                            (0x2c2, initial([530, 14, 0], &[(99, 3)])),
                            (0x3e, location_body(530, 32.0)),
                        ],
                        0,
                    )
                    .await?
                    .await??;
                let destination = loop {
                    gameplay.service()?;
                    if let Some(WorldTransfer::NewWorld(location)) = gameplay.take_world_transfer()
                    {
                        break location;
                    }
                    tokio::task::yield_now().await;
                };
                gameplay.replace_world(destination)?;
                let state = gameplay
                    .world()
                    .ok_or("missing replacement")?
                    .world_state_values();
                assert_eq!(state.location(), [530, 14, 0]);
                assert_eq!(state.value(88), 9);
                assert_eq!(state.value(99), 3);
                assert_eq!(
                    resolve_world_state_zone_sounds(&rows, location, |key| state.value(key)),
                    None
                );
                assert!(gameplay.unhandled_packets().is_empty());
                gameplay.disconnect();
                Ok::<(), TestError>(())
            })
            .await?
        })
}

fn initial(location: [u32; 3], values: &[(u32, u32)]) -> Vec<u8> {
    let mut payload: Vec<_> = location.into_iter().flat_map(u32::to_le_bytes).collect();
    payload.extend((values.len() as u16).to_le_bytes());
    for &(field, value) in values {
        payload.extend(field.to_le_bytes());
        payload.extend(value.to_le_bytes());
    }
    payload
}
