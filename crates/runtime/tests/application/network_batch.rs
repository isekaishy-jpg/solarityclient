//! Producer refill cannot extend the packet batch being dispatched by a frame.

use super::{ActiveGameplayNetwork, GameplayNetworkEvent, RuntimeGameplayCoordinator};
use crate::application::player_control::RuntimePlayerControl;
use crate::test_network::{TestError, WorldServer};
use glam::Vec3;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
use solarity_systems::project_object_fields;
use tokio::sync::mpsc;

#[test]
fn packet_callback_refill_waits_for_the_next_service() -> Result<(), TestError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let (server, mut network) = WorldServer::connect().await?;
        let mut packets = Vec::new();
        for state in [0_u32, 1] {
            let mut body = 1_u32.to_le_bytes().to_vec();
            body.extend([0, 1, 9, 1]); // values block, GUID 9, one field mask
            body.extend((1_u32 << 17).to_le_bytes());
            body.extend(state.to_le_bytes());
            server.exchange(vec![(0xa9, body)], 0).await?.await??;
            packets.push(network.receive_packet().await?);
        }
        let refill = packets.pop().ok_or("missing refill packet")?;
        let first = packets.pop().ok_or("missing initial packet")?;
        let (sender, receiver) = mpsc::channel(8);
        let (commands, _writer) = mpsc::channel(8);
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.,
        ));
        let fields = [(8, 42), (17, 1)];
        world.create_object(9, ObjectKind::GameObject, None, fields)?;
        project_object_fields(&mut world, 9, fields)?;
        let player = world.object_identity(7).ok_or("missing local player")?;
        let mut gameplay = RuntimeGameplayCoordinator::with_test_world(world);
        gameplay.player_control = Some(RuntimePlayerControl::new(player));
        gameplay.active = Some(ActiveGameplayNetwork {
            receiver,
            commands,
            task: runtime.spawn(std::future::pending()),
        });
        sender.try_send(Ok(GameplayNetworkEvent::Packet(first)))?;
        let mut refill = Some(refill);
        assert_eq!(
            gameplay.service_with_game_objects(&mut |_, _, _, _| {
                if let Some(packet) = refill.take() {
                    assert!(
                        sender
                            .try_send(Ok(GameplayNetworkEvent::Packet(packet)))
                            .is_ok()
                    );
                }
                Ok(())
            })?,
            1
        );
        assert!(
            refill.is_none(),
            "the producer refilled during packet dispatch"
        );
        assert_eq!(
            gameplay
                .active
                .as_ref()
                .ok_or("missing active transport")?
                .receiver
                .len(),
            1
        );
        assert_eq!(gameplay.service()?, 1);
        assert_eq!(gameplay.service()?, 0);
        gameplay.disconnect();
        Ok(())
    })
}
