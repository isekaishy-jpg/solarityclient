//! Real SRP and world-header authentication with independent realm-list ownership.

use super::*;
use crate::application::{RuntimeWorldCoordinator, RuntimeWorldPoll};
use crate::test_network::{TestError, authenticate_test_world};
use solarity_network::{
    GruntIntegrity, GruntLoginOptions, LoginLocale, TcpEndpoint, WorldAddonManifest,
};
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use wow_login_messages::Message;
use wow_login_messages::all::Population;
use wow_login_messages::version_8::opcodes::ClientOpcodeMessage;
use wow_login_messages::version_8::{
    AccountFlag, CMD_AUTH_LOGON_CHALLENGE_Server, CMD_AUTH_LOGON_CHALLENGE_Server_SecurityFlag,
    CMD_AUTH_LOGON_PROOF_Server, CMD_REALM_LIST_Server, Realm, Realm_RealmFlag,
};
use wow_srp::normalized_string::NormalizedString;
use wow_srp::server::SrpVerifier;
use wow_srp::{GENERATOR, LARGE_SAFE_PRIME_LITTLE_ENDIAN, PublicKey};

#[test]
fn realm_refresh_survives_world_login_and_realm_switch() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let login_listener = TcpListener::bind("127.0.0.1:0").await?;
                let world_listener = TcpListener::bind("127.0.0.1:0").await?;
                let address = login_listener.local_addr()?;
                let world_address = world_listener.local_addr()?.to_string();
                let (key_tx, key_rx) = oneshot::channel();
                let login_server =
                    tokio::spawn(async move {
                        let (mut stream, _) = login_listener.accept().await?;
                        let key = authenticate_realmd(&mut stream).await?;
                        key_tx.send(key).map_err(|_| "world key receiver closed")?;
                        for count in [1, 2] {
                            let request = ClientOpcodeMessage::tokio_read(&mut stream).await?;
                            assert!(matches!(request, ClientOpcodeMessage::CMD_REALM_LIST(_)));
                            CMD_REALM_LIST_Server {
                                realms: vec![Realm {
                        realm_type: wow_login_messages::version_2::RealmType::PlayerVsEnvironment,
                        locked: false, flag: Realm_RealmFlag::empty(),
                        name: "Realm continuity".to_owned(), address: world_address.clone(),
                        population: Population::Other(0.5), number_of_characters_on_realm: count,
                        category: wow_login_messages::version_2::RealmCategory::One, realm_id: 7,
                    }],
                            }
                            .tokio_write(&mut stream)
                            .await?;
                        }
                        // Retain realmd until the explicit full disconnect.
                        let mut byte = [0];
                        assert_eq!(stream.read(&mut byte).await?, 0);
                        Ok::<(), TestError>(())
                    });
                let world_server = tokio::spawn(async move {
                    let key = key_rx.await?;
                    for _ in 0..2 {
                        let (mut stream, _) = world_listener.accept().await?;
                        let _crypto = authenticate_test_world(&mut stream, key).await?;
                        let mut byte = [0];
                        assert_eq!(stream.read(&mut byte).await?, 0);
                    }
                    Ok::<(), TestError>(())
                });
                let mut login = RuntimeLoginCoordinator::new(LoginConfiguration::new(
                    TcpEndpoint::new(address.ip().to_string(), address.port())?,
                    GruntLoginOptions::new(LoginLocale::EnUs, -240, Ipv4Addr::LOCALHOST),
                ));
                login.begin(&Handle::current(), "testaccount", "hunter2")?;
                loop {
                    if login.poll()? == RuntimeLoginPoll::Authenticated {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                let mut world = RuntimeWorldCoordinator::new();
                for pass in 0..2 {
                    let authenticated = login
                        .authenticated()
                        .ok_or("realmd lost after world login")?;
                    let realm = authenticated
                        .realms()
                        .by_id(7)
                        .ok_or("realm absent")?
                        .clone();
                    world.begin(
                        &Handle::current(),
                        authenticated.world_identity(),
                        realm,
                        WorldAddonManifest::empty(),
                    )?;
                    loop {
                        if world.poll()? == RuntimeWorldPoll::CharacterScreenReady {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                    assert_eq!(login.state(), RuntimeLoginState::Authenticated);
                    if pass == 0 {
                        login.refresh_realms(&Handle::current())?;
                        assert!(login.authenticated().is_none());
                        assert_eq!(
                            login
                                .realms()
                                .ok_or("cached rows lost")?
                                .by_id(7)
                                .ok_or("cached realm")?
                                .character_count(),
                            1
                        );
                        loop {
                            if login.poll()? == RuntimeLoginPoll::RealmDirectoryUpdated {
                                break;
                            }
                            tokio::task::yield_now().await;
                        }
                        assert_eq!(
                            login
                                .realms()
                                .ok_or("updated rows lost")?
                                .by_id(7)
                                .ok_or("updated realm")?
                                .character_count(),
                            2
                        );
                    }
                    world.disconnect();
                }
                login.disconnect();
                assert!(login.realms().is_none());
                login_server.await??;
                world_server.await??;
                Ok::<(), TestError>(())
            })
            .await?
        })
}

async fn authenticate_realmd(stream: &mut TcpStream) -> Result<[u8; 40], TestError> {
    let ClientOpcodeMessage::CMD_AUTH_LOGON_CHALLENGE(challenge) =
        ClientOpcodeMessage::tokio_read(&mut *stream).await?
    else {
        return Err("expected login challenge".into());
    };
    assert_eq!(challenge.account_name, "TESTACCOUNT");
    let proof = SrpVerifier::from_username_and_password(
        NormalizedString::new("testaccount")?,
        NormalizedString::new("hunter2")?,
    )
    .into_proof();
    let salt = [
        0xBA, 0xA3, 0x1E, 0x99, 0xA0, 0x0B, 0x21, 0x57, 0xFC, 0x37, 0x3F, 0xB3, 0x69, 0xCD, 0xD2,
        0xF1,
    ];
    CMD_AUTH_LOGON_CHALLENGE_Server::Success {
        crc_salt: salt,
        generator: vec![GENERATOR],
        large_safe_prime: LARGE_SAFE_PRIME_LITTLE_ENDIAN.to_vec(),
        salt: *proof.salt(),
        security_flag: CMD_AUTH_LOGON_CHALLENGE_Server_SecurityFlag::empty(),
        server_public_key: *proof.server_public_key(),
    }
    .tokio_write(&mut *stream)
    .await?;
    let ClientOpcodeMessage::CMD_AUTH_LOGON_PROOF(client) =
        ClientOpcodeMessage::tokio_read(&mut *stream).await?
    else {
        return Err("expected login proof".into());
    };
    assert_eq!(
        client.crc_hash,
        Build12340WindowsIntegrity.proof(salt, client.client_public_key)?
    );
    let (server, server_proof) = proof.into_server(
        PublicKey::from_le_bytes(client.client_public_key)?,
        client.client_proof,
    )?;
    CMD_AUTH_LOGON_PROOF_Server::Success {
        account_flag: AccountFlag::empty(),
        hardware_survey_id: 0,
        server_proof,
        unknown: 0,
    }
    .tokio_write(&mut *stream)
    .await?;
    Ok(*server.session_key())
}
