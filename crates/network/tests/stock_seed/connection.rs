//! External stock-compatibility tests for the selected-realm world transition.

use std::error::Error;

use solarity_network::{
    AccountExpansion, WorldAddon, WorldAddonManifest, WorldAuthProgress, WorldConnection,
};
use tokio::io::DuplexStream;
use wow_srp::normalized_string::NormalizedString;
use wow_srp::wrath_header::ProofSeed;
use wow_world_messages::wrath::opcodes::{ClientOpcodeMessage, ServerOpcodeMessage};
use wow_world_messages::wrath::{
    BillingPlanFlags, Expansion, SMSG_AUTH_CHALLENGE, SMSG_AUTH_RESPONSE,
};

use super::authentication::authenticated_identity_and_realm;

/// World authentication preserves legacy fields, add-ons, queue state, and header encryption.
#[test]
fn selected_realm_authenticates_an_encrypted_world_session()
-> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, server) = tokio::io::duplex(4_096);
        let server_task = tokio::spawn(emulate_worldserver(server, session_key));
        let addons = WorldAddonManifest::new(vec![
            WorldAddon::new("Blizzard_AuctionUI", true, 0x1122_3344, 0x5566_7788)?,
            WorldAddon::new("QuestHelper", false, 0xAABB_CCDD, 0xEEFF_0011)?,
        ])?;

        let progress = WorldConnection::authenticate(client, identity, &realm, addons).await?;
        let queue = match progress {
            WorldAuthProgress::Queued(queue) => queue,
            WorldAuthProgress::Authenticated(_) => {
                return Err("fixture world omitted the queue response".into());
            }
        };
        assert_eq!(queue.position(), 3);
        assert!(queue.realm_has_free_character_migration());

        let session = match queue.advance().await? {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => {
                return Err("fixture world sent a second queue response".into());
            }
        };
        assert_eq!(session.account_name(), "TESTACCOUNT");
        assert_eq!(session.realm_id(), 7);
        assert_eq!(session.info().billing_time(), 86_400);
        assert_eq!(session.info().billing_flags(), BillingPlanFlags::FREE_TRIAL);
        assert_eq!(session.info().billing_rested(), 4_200);
        assert_eq!(
            session.info().expansion(),
            AccountExpansion::WrathOfTheLichKing
        );
        drop(session);

        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

async fn emulate_worldserver(
    mut stream: DuplexStream,
    session_key: [u8; 40],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let server_seed = ProofSeed::new();
    ServerOpcodeMessage::from(SMSG_AUTH_CHALLENGE {
        unknown1: 1,
        server_seed: server_seed.seed(),
        seed: [0x5A; 32],
    })
    .tokio_write_unencrypted_server(&mut stream)
    .await?;

    let auth_session = match ClientOpcodeMessage::tokio_read_unencrypted(&mut stream).await? {
        ClientOpcodeMessage::CMSG_AUTH_SESSION(auth_session) => *auth_session,
        message => return Err(format!("unexpected world proof packet: {message}").into()),
    };
    assert_eq!(auth_session.client_build, 12_340);
    assert_eq!(auth_session.login_server_id, 0);
    assert_eq!(auth_session.username, "TESTACCOUNT");
    assert_eq!(auth_session.login_server_type, 0);
    assert_eq!(auth_session.region_id, 0);
    assert_eq!(auth_session.battleground_id, 0);
    assert_eq!(auth_session.realm_id, 7);
    assert_eq!(auth_session.dos_response, 0);
    assert_eq!(auth_session.addon_info, expected_addon_bytes());

    let mut crypto = server_seed.into_server_header_crypto(
        &NormalizedString::new("testaccount")?,
        session_key,
        auth_session.client_proof,
        auth_session.client_seed,
    )?;
    ServerOpcodeMessage::from(SMSG_AUTH_RESPONSE::AuthWaitQueue {
        queue_position: 3,
        realm_has_free_character_migration: true,
    })
    .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
    .await?;
    ServerOpcodeMessage::from(SMSG_AUTH_RESPONSE::AuthOk {
        billing_flags: BillingPlanFlags::new(BillingPlanFlags::FREE_TRIAL),
        billing_rested: 4_200,
        billing_time: 86_400,
        expansion: Expansion::WrathOfTheLichKing,
    })
    .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
    .await?;
    Ok(())
}

fn expected_addon_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(b"Blizzard_AuctionUI\0");
    bytes.push(1);
    bytes.extend_from_slice(&0x1122_3344_u32.to_le_bytes());
    bytes.extend_from_slice(&0x5566_7788_u32.to_le_bytes());
    bytes.extend_from_slice(b"QuestHelper\0");
    bytes.push(0);
    bytes.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
    bytes.extend_from_slice(&0xEEFF_0011_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes
}

fn runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error + Send + Sync>> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}
