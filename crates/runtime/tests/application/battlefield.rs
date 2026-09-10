//! Compare the context reducer with original instructions and real packet dispatch.

use super::*;

#[test]
fn battlefield_setup_packets_install_context_before_active_world_frames()
-> Result<(), crate::test_network::TestError> {
    use super::super::*;
    use crate::test_network::WorldServer;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut body = vec![0; 44];
            body[4] = 1;
            body[19] = 3;
            body[23] = 42;
            server.exchange(vec![(0x2d4, body)], 0).await?.await??;
            let setup = network.receive_packet().await?;
            let mut gameplay = RuntimeGameplayCoordinator::new();
            gameplay.battlefield_maps = [(42, true)].into_iter().collect();
            gameplay.begin(&Handle::current(), network, vec![setup])?;
            assert!(gameplay.player_ui.arena);
            assert_eq!(gameplay.battlefield.active_queue, Some(0));
            assert!(gameplay.unhandled_packets().is_empty());
            gameplay.disconnect();
            Ok(())
        })
}

#[test]
fn battlefield_context_matches_native_packet_and_lookup_branches()
-> Result<(), crate::test_network::TestError> {
    use crate::test_network::WorldServer;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let maps = [(40, false), (41, false), (42, true)].into_iter().collect();
            let rows = include_str!("../fixtures/battlefield_status_native.txt")
                .lines()
                .filter_map(|line| line.strip_prefix("context "))
                .map(|line| line.split_whitespace().collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let packets = rows
                .iter()
                .map(|row| {
                    let body = (0..row[2].len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&row[2][i..i + 2], 16))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok((0x2d4, body))
                })
                .collect::<Result<Vec<_>, std::num::ParseIntError>>()?;
            let sent = server.exchange(packets, 0).await?;
            for row in &rows {
                let active = row[0].parse::<u32>()?;
                let mut state = BattlefieldState {
                    active_queue: (active != u32::MAX).then_some(active),
                };
                let mut arena = row[1].parse::<u32>()? == 4;
                state.receive(
                    network
                        .receive_packet()
                        .await?
                        .battlefield_status()?
                        .ok_or("status")?,
                    &maps,
                    &mut arena,
                );
                assert_eq!(
                    state.active_queue.unwrap_or(u32::MAX),
                    row[3].parse::<u32>()?,
                    "{row:?}"
                );
                assert_eq!(arena, row[4].parse::<u32>()? == 4, "{row:?}");
            }
            sent.await??;
            assert_eq!(rows.len(), 864);
            Ok(())
        })
}

#[test]
fn battlefield_packets_update_arena_context_across_world_transfers()
-> Result<(), crate::test_network::TestError> {
    use super::super::*;
    use crate::test_network::WorldServer;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let (server, mut network) = WorldServer::connect().await?;
        let (sender, receiver) = mpsc::channel(8);
        let (commands, _writer) = mpsc::channel(8);
        let mut gameplay = RuntimeGameplayCoordinator::with_test_world(ActiveWorld::enter(
            WorldBootstrap::new(WorldMapId::new(0), 1, "Arena", Vec3::ZERO, 0.),
        ));
        gameplay.battlefield_maps = [(0, false), (42, true)].into_iter().collect();
        gameplay.active = Some(ActiveGameplayNetwork {
            receiver,
            commands,
            task: runtime.spawn(std::future::pending()),
        });
        for (queue, map, expected) in [
            (0u32, 42u32, true),
            (1, 999, true),
            (0, 0, false),
            (1, 42, true),
        ] {
            let mut body = vec![0; 44];
            body[..4].copy_from_slice(&queue.to_le_bytes());
            body[4] = 1;
            body[19] = 3;
            body[23..27].copy_from_slice(&map.to_le_bytes());
            server.exchange(vec![(0x2d4, body)], 0).await?.await??;
            sender.try_send(Ok(GameplayNetworkEvent::Packet(
                network.receive_packet().await?,
            )))?;
            assert_eq!(gameplay.service()?, 1);
            assert_eq!(gameplay.player_ui.arena, expected);
        }
        assert!(gameplay.unhandled_packets().is_empty());
        server
            .exchange(vec![(0x236, vec![0; 20])], 0)
            .await?
            .await??;
        let location = network
            .receive_packet()
            .await?
            .world_location()?
            .ok_or("location")?;
        gameplay.replace_world(location)?;
        assert!(gameplay.player_ui.arena);
        assert_eq!(gameplay.battlefield.active_queue, Some(1));
        for queue in [0u32, 1] {
            let mut body = vec![0; 12];
            body[..4].copy_from_slice(&queue.to_le_bytes());
            server.exchange(vec![(0x2d4, body)], 0).await?.await??;
            sender.try_send(Ok(GameplayNetworkEvent::Packet(
                network.receive_packet().await?,
            )))?;
            assert_eq!(gameplay.service()?, 1);
            assert_eq!(gameplay.player_ui.arena, queue == 0);
        }
        gameplay.player_ui.arena = true;
        gameplay.battlefield.active_queue = Some(0);
        gameplay.disconnect();
        assert!(!gameplay.player_ui.arena);
        assert_eq!(gameplay.battlefield.active_queue, None);
        Ok(())
    })
}
