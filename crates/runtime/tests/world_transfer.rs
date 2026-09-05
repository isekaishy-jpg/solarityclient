//! Transfer lifetime tests through an authenticated TCP session and the real pump.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use solarity_ecs::PlayerViewState;
use solarity_network::WorldTransfer;
use solarity_runtime::{
    RuntimeGameplayCoordinator, RuntimeWorldTransferCoordinator, RuntimeWorldTransferEffect,
};
use wow_world_messages::wrath::opcodes::ClientOpcodeMessage;

use transfer_world_server::{TestError, WorldServer, location_body};

/// Abort and rejected destination packets never replace the published world or
/// send an ACK; malformed NEW_WORLD also leaves the encrypted session usable.
#[test]
fn transfer_abort_and_invalid_destinations_preserve_the_live_world() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (server, session) = WorldServer::connect().await?;
            let mut gameplay = RuntimeGameplayCoordinator::new();
            gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
            let mut transfer = RuntimeWorldTransferCoordinator::new();
            let mut abort = 530_u32.to_le_bytes().to_vec();
            abort.push(2);
            let response = server.exchange(vec![
                (0xA9, remote_object_create()),
                (0x3F, 530_u32.to_le_bytes().to_vec()),
                (0x40, abort),
                (0x3E, location_body(999, 99.0)),
                (0x3E, vec![0; 19]),
                (0x236, location_body(0, 111.0)),
                (0x390, 93_u32.to_le_bytes().to_vec()),
            ], 1).await?;
            let packet = next_transfer(&mut gameplay).await?;
            assert!(matches!(transfer.receive(packet, 0, true), RuntimeWorldTransferEffect::OpenCard { .. }));
            let packet = next_transfer(&mut gameplay).await?;
            assert_eq!(transfer.receive(packet, 0, true), RuntimeWorldTransferEffect::Abort { map_id: 530, reason: 2, argument: None });
            assert!(!transfer.holds_loading_card());
            let packet = next_transfer(&mut gameplay).await?;
            assert_eq!(transfer.receive(packet, 0, false), RuntimeWorldTransferEffect::InvalidMap(999));
            assert!(transfer.take_deferred_replacement().is_none());
            let packet = next_transfer(&mut gameplay).await?;
            assert!(matches!(packet, WorldTransfer::VerifyWorld(_)));
            assert_eq!(transfer.receive(packet, 0, true), RuntimeWorldTransferEffect::None);
            let world = gameplay.world().ok_or("missing retained world")?;
            assert_eq!(world.map_id().value(), 0);
            assert_eq!(world.local_player_transform()?.position().x, 12.0);
            assert!(world.entity_by_guid(9).is_some());
            let response = response.await??;
            assert!(matches!(&response[0], ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response) if response.time_sync == 93));
            Ok::<(), TestError>(())
        }).await?
    })
}

/// Native 0x403D10 schedules both callbacks against the latest shared destination;
/// 0x403B70 replaces all objects and sends one ACK after each map load.
#[test]
fn repeated_new_world_replaces_objects_and_acknowledges_each_completed_map() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (server, session) = WorldServer::connect().await?;
            let mut gameplay = RuntimeGameplayCoordinator::new();
            gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
            let mut transfer = RuntimeWorldTransferCoordinator::new();
            let mut responses = server.exchange(vec![
                (0xA9, remote_object_create()),
                (0x3F, 530_u32.to_le_bytes().to_vec()),
                (0x3E, location_body(530, 21.0)),
                (0x3E, location_body(571, 32.0)),
                (0x390, 77_u32.to_le_bytes().to_vec()),
            ], 3).await?;
            let pending = next_transfer(&mut gameplay).await?;
            let retained_view = PlayerViewState::new(11.5, 0.24, 0.0, 4);
            {
                let world = gameplay.world().ok_or("missing initial camera world")?;
                **world.storage().get::<&mut PlayerViewState>(world.local_player())? = retained_view;
            }
            assert!(matches!(transfer.receive(pending, 0, true), RuntimeWorldTransferEffect::OpenCard { map_id: 530, .. }));
            assert!(gameplay.world().ok_or("missing initial world")?.entity_by_guid(9).is_some());
            assert!(transfer.holds_loading_card());
            for _ in 0..2 {
                let packet = next_transfer(&mut gameplay).await?;
                assert_eq!(transfer.receive(packet, 0, true), RuntimeWorldTransferEffect::None);
            }
            for _ in 0..2 {
                let replacement = transfer.take_deferred_replacement().ok_or("missing queued callback")?;
                assert_eq!(replacement.location().map_id(), 571);
                assert_eq!(replacement.location().x(), 32.0);
                assert!(replacement.requires_acknowledgement());
                gameplay.replace_world(replacement.location())?;
                let world = gameplay.world().ok_or("missing replacement world")?;
                assert_eq!(world.map_id().value(), 571);
                assert_eq!(world.local_player_view()?, retained_view);
                assert_eq!(world.local_player_transform()?.position().x, 32.0);
                assert_eq!(world.local_player_identity().ok_or("missing player name")?.name(), "Transferfixture");
                assert!(world.entity_by_guid(9).is_none());
                assert!(world.local_player_presentation().is_none());
                assert!(gameplay.world_entry_ground_contact_pending());
                assert!(!transfer.complete_player());
                assert!(matches!(responses.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty)));
                assert!(gameplay.acknowledge_world_transfer()?);
                transfer.complete_map();
            }
            let responses = responses.await??;
            assert_eq!(responses.iter().filter(|message| matches!(message, ClientOpcodeMessage::MSG_MOVE_WORLDPORT_ACK)).count(), 2);
            assert!(responses.iter().any(|message| matches!(message, ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response) if response.time_sync == 77)));
            assert!(transfer.take_deferred_replacement().is_none());
            assert!(!transfer.holds_loading_card());
            assert!(transfer.complete_player());
            assert!(!transfer.complete_player());

            // A same-map verify packet must preserve the world even if it
            // carries a different position. A different map takes the immediate
            // replacement path but adds no ACK to the encrypted stream.
            let published = server.exchange(vec![
                (0xA9, remote_object_create()),
                (0x236, location_body(571, 99.0)),
                (0x236, location_body(530, 42.0)),
            ], 0).await?;
            published.await??;
            let same = next_transfer(&mut gameplay).await?;
            assert_eq!(transfer.receive(same, 571, true), RuntimeWorldTransferEffect::None);
            let world = gameplay.world().ok_or("missing retained world")?;
            assert!(world.entity_by_guid(9).is_some());
            assert_eq!(world.local_player_transform()?.position().x, 32.0);
            let different = next_transfer(&mut gameplay).await?;
            let RuntimeWorldTransferEffect::Replace(replacement) = transfer.receive(different, 571, true) else {
                return Err("different-map verify did not replace immediately".into());
            };
            assert!(!replacement.requires_acknowledgement());
            gameplay.replace_world(replacement.location())?;
            transfer.complete_map();
            let response = server.exchange(vec![(0x390, 88_u32.to_le_bytes().to_vec())], 1).await?.await??;
            assert!(matches!(&response[0], ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response) if response.time_sync == 88));
            gameplay.disconnect();
            transfer.disconnect();
            assert!(gameplay.world().is_none());
            assert!(!transfer.holds_loading_card());
            Ok::<(), TestError>(())
        }).await?
    })
}

/// Runs real main-thread dispatch until its next protocol transfer boundary.
async fn next_transfer(
    gameplay: &mut RuntimeGameplayCoordinator,
) -> Result<WorldTransfer, TestError> {
    loop {
        gameplay.service()?;
        if let Some(transfer) = gameplay.take_world_transfer() {
            return Ok(transfer);
        }
        tokio::task::yield_now().await;
    }
}

/// A nonlocal base-object create with one authored OBJECT_FIELD_ENTRY word.
fn remote_object_create() -> Vec<u8> {
    let mut body = 1_u32.to_le_bytes().to_vec();
    body.extend_from_slice(&[2, 1, 9, 0]);
    body.extend_from_slice(&0_u16.to_le_bytes());
    body.push(1);
    body.extend_from_slice(&(1_u32 << 3).to_le_bytes());
    body.extend_from_slice(&55_u32.to_le_bytes());
    body
}
