//! Received control/stance packets through the authenticated world pump.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use solarity_runtime::RuntimeGameplayCoordinator;
use transfer_world_server::{TestError, WorldServer, location_body};

#[test]
fn control_waits_for_create_and_stance_survives_independent_unit_bytes() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (server, session) = WorldServer::connect().await?;
            let mut gameplay = RuntimeGameplayCoordinator::new();
            gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
            assert_eq!(gameplay.active_mover_guid(), Some(8));
            // The transfer boundary lets the real dispatcher expose each
            // intermediate state without reaching into private coordinator state.
            let boundary = || (0x236, location_body(0, 12.));
            server.exchange_raw(vec![(0x159, vec![1, 8, 0]), boundary()], 0).await?.await??;
            next_boundary(&mut gameplay).await?;
            assert_eq!(gameplay.player_control_enabled(), Some(true));
            server.exchange_raw(vec![(0xa9, player_create()), (0x29d, vec![255]), boundary()], 0).await?.await??;
            next_boundary(&mut gameplay).await?;
            assert_eq!(gameplay.player_control_enabled(), Some(false));
            assert_eq!(gameplay.active_mover_guid(), Some(0));
            assert_eq!(gameplay.world().ok_or("world")?.local_player_stand_state()?, 255);
            server.exchange_raw(vec![(0x159, vec![1, 8, 255]), (0x29d, vec![1]), (0xa9, player_create()), boundary()], 0).await?.await??;
            next_boundary(&mut gameplay).await?;
            let world = gameplay.world().ok_or("world")?;
            assert_eq!(world.local_player_stand_state()?, 1);
            assert_eq!(world.local_player_presentation().ok_or("unit presentation")?.stand_state(), 0);
            assert_eq!(gameplay.active_mover_guid(), Some(8));
            assert_eq!(gameplay.player_control_enabled(), Some(true));
            let replies = server.exchange(vec![(0x390, 98_u32.to_le_bytes().to_vec())], 1).await?.await??;
            assert!(matches!(&replies[0], wow_world_messages::wrath::opcodes::ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response) if response.time_sync == 98));
            gameplay.disconnect();
            assert_eq!(gameplay.active_mover_guid(), None);
            assert_eq!(gameplay.player_control_enabled(), None);
            Ok::<(), TestError>(())
        }).await?
    })
}

async fn next_boundary(gameplay: &mut RuntimeGameplayCoordinator) -> Result<(), TestError> {
    loop {
        gameplay.service()?;
        if gameplay.take_world_transfer().is_some() {
            return Ok(());
        }
        tokio::task::yield_now().await;
    }
}

fn player_create() -> Vec<u8> {
    let mut body = 1_u32.to_le_bytes().to_vec();
    body.extend_from_slice(&[2, 1, 8, 4]);
    body.extend_from_slice(&0_u16.to_le_bytes());
    body.push(1);
    body.extend_from_slice(&(1_u32 << 3).to_le_bytes());
    body.extend_from_slice(&0_u32.to_le_bytes());
    body
}
