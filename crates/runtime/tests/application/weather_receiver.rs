//! Active packet dispatch retains weather order and clears stale map updates.

use super::*;
use crate::test_network::{TestError, WorldServer};

#[test]
fn weather_receiver_preserves_packet_order_and_resets_on_world_replacement() -> Result<(), TestError>
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let (server, mut network) = WorldServer::connect().await?;
        let (sender, receiver) = mpsc::channel(8);
        let (commands, _writer) = mpsc::channel(8);
        let mut gameplay = RuntimeGameplayCoordinator::new();
        gameplay.world = Some(ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            1,
            "Weather",
            Vec3::ZERO,
            0.,
        )));
        gameplay.active = Some(ActiveGameplayNetwork {
            receiver,
            commands,
            task: runtime.spawn(std::future::pending()),
        });
        let updates = [
            solarity_network::WorldWeatherUpdate {
                weather_id: 1,
                grade: 0.7,
                instant: false,
            },
            solarity_network::WorldWeatherUpdate {
                weather_id: 2,
                grade: 0.1,
                instant: true,
            },
        ];
        for update in updates {
            let mut body = update.weather_id.to_le_bytes().to_vec();
            body.extend_from_slice(&update.grade.to_le_bytes());
            body.push(u8::from(update.instant));
            server.exchange(vec![(0x2f4, body)], 0).await?.await??;
            sender.try_send(Ok(GameplayNetworkEvent::Packet(
                network.receive_packet().await?,
            )))?;
        }
        assert_eq!(gameplay.service()?, 2);
        assert!(gameplay.unhandled_packets().is_empty());
        assert_eq!(
            gameplay.take_weather_update().map(|entry| entry.0),
            Some(updates[0])
        );
        let mut body = 1_u32.to_le_bytes().to_vec();
        body.extend_from_slice(&[0; 16]);
        server.exchange(vec![(0x236, body)], 0).await?.await??;
        let location = network
            .receive_packet()
            .await?
            .world_location()?
            .ok_or("location")?;
        gameplay.replace_world(location)?;
        assert!(gameplay.take_weather_update().is_none());
        gameplay.weather_updates.push_back((updates[1], 0));
        gameplay.disconnect();
        assert!(gameplay.take_weather_update().is_none());
        Ok(())
    })
}
