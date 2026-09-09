//! Real encrypted session continuity through stock's logout packet boundary.

use super::*;
use crate::test_network::{TestError, WorldServer};
use solarity_network::CharacterLoginProgress;
use wow_world_messages::wrath::opcodes::ServerOpcodeMessage;
use wow_world_messages::wrath::{Character, Class, Gender, Race, SMSG_CHAR_ENUM};

#[test]
fn logout_preserves_cipher_metadata_and_character_screen_transport() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let account = session.account_name().to_owned();
                let realm = session.realm_id();
                let info = session.info();
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&Handle::current(), session, Vec::new())?;
                for (request, opcode) in [
                    (WorldLogoutRequest::Request, 0x4b),
                    (WorldLogoutRequest::Cancel, 0x4e),
                    (WorldLogoutRequest::Force, 0x4a),
                ] {
                    let sent = server.exchange_raw(Vec::new(), 1).await?;
                    assert!(gameplay.send_logout_action(request)?);
                    assert_eq!(sent.await??, vec![(opcode, Vec::new())]);
                }
                // Fill the command queue before COMPLETE can arrive. Handoff
                // must keep servicing the writer while it waits for queue capacity.
                for state in 0..WRITER_CHANNEL_CAPACITY {
                    gameplay
                        .active
                        .as_ref()
                        .ok_or("writer")?
                        .commands
                        .try_send(WorldWriterCommand::StandState(state as u32 % 8))?;
                }
                let drained = server
                    .exchange_raw(
                        vec![
                            (0x4c, vec![0, 0, 0, 0, 0]),
                            (0x4f, Vec::new()),
                            (0x4d, Vec::new()),
                            (0x3b, vec![0]),
                        ],
                        WRITER_CHANNEL_CAPACITY,
                    )
                    .await?
                    .await??;
                assert_eq!(
                    drained,
                    (0..WRITER_CHANNEL_CAPACITY)
                        .map(|state| (0x101, (state as u32 % 8).to_le_bytes().to_vec()))
                        .collect::<Vec<_>>()
                );
                let mut updates = Vec::new();
                let mut session = loop {
                    gameplay.service()?;
                    if let Some(update) = gameplay.take_logout_update() {
                        updates.push(update);
                    }
                    if let Some(session) = gameplay.take_logged_out_session() {
                        break session;
                    }
                    tokio::task::yield_now().await;
                };
                assert_eq!(
                    updates,
                    vec![
                        WorldLogout::Response {
                            reason: 0,
                            instant: false
                        },
                        WorldLogout::CancelAcknowledged
                    ]
                );
                assert!(gameplay.unhandled_packets().is_empty());
                assert!(
                    gameplay.world().is_some(),
                    "world remains available for orderly retirement"
                );
                gameplay.disconnect();
                assert_eq!(session.account_name(), account);
                assert_eq!(session.realm_id(), realm);
                assert_eq!(session.info(), info);
                // A packet already following COMPLETE remains unread and decrypts
                // using the same receive state after character-screen ownership resumes.
                assert_eq!(
                    session
                        .receive_packet()
                        .await?
                        .character_directory()?
                        .ok_or("directory")?
                        .entries()
                        .len(),
                    0
                );
                let sent = server.exchange_raw(Vec::new(), 3).await?;
                session.ready_for_account_data_times().await?;
                session.request_character_directory().await?;
                session.request_realm_split_info().await?;
                let requests = sent.await??;
                assert_eq!(
                    requests.iter().map(|packet| packet.0).collect::<Vec<_>>(),
                    vec![0x4ff, 0x37, 0x38c]
                );
                let mut enumeration = Vec::new();
                ServerOpcodeMessage::from(SMSG_CHAR_ENUM {
                    characters: vec![Character {
                        guid: wow_world_messages::Guid::new(8),
                        name: "Returned".to_owned(),
                        race: Race::Human,
                        class: Class::Warrior,
                        gender: Gender::Male,
                        ..Character::default()
                    }],
                })
                .tokio_write_unencrypted_server(&mut enumeration)
                .await?;
                server
                    .exchange_raw(vec![(0x3b, enumeration[4..].to_vec())], 0)
                    .await?
                    .await??;
                let directory = session
                    .receive_packet()
                    .await?
                    .character_directory()?
                    .ok_or("returned directory")?;
                let sent = server
                    .exchange_raw(
                        vec![(
                            0x236,
                            [
                                0u32.to_le_bytes(),
                                12f32.to_le_bytes(),
                                0f32.to_le_bytes(),
                                0f32.to_le_bytes(),
                                0f32.to_le_bytes(),
                            ]
                            .concat(),
                        )],
                        1,
                    )
                    .await?;
                let session = match session
                    .login_character(directory.entries().first().ok_or("returned character")?)
                    .await?
                    .advance()
                    .await?
                {
                    CharacterLoginProgress::Entered(session) => session,
                    _ => return Err("second world entry did not complete".into()),
                };
                assert_eq!(sent.await??, vec![(0x3d, 8u64.to_le_bytes().to_vec())]);
                assert_eq!(session.character_name(), "Returned");
                gameplay.begin(&Handle::current(), session, Vec::new())?;
                server
                    .exchange_raw(vec![(0x4d, Vec::new())], 0)
                    .await?
                    .await??;
                let session = loop {
                    gameplay.service()?;
                    if let Some(session) = gameplay.take_logged_out_session() {
                        break session;
                    }
                    tokio::task::yield_now().await;
                };
                gameplay.disconnect();
                let mut coordinator =
                    crate::application::world_coordinator::RuntimeWorldCoordinator::new();
                coordinator.resume_character_screen(session)?;
                assert_eq!(
                    coordinator.poll()?,
                    crate::application::world_coordinator::RuntimeWorldPoll::CharacterScreenReady
                );
                drop(server);
                loop {
                    match coordinator.poll() {
                        Err(
                            crate::application::world_coordinator::RuntimeWorldError::Disconnected,
                        ) => break,
                        Ok(_) => tokio::task::yield_now().await,
                        Err(error) => return Err(error.into()),
                    }
                }
                assert!(coordinator.authenticated().is_none());
                Ok::<(), TestError>(())
            })
            .await?
        })
}

#[test]
fn active_world_peer_closure_reaches_the_disconnect_boundary() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&Handle::current(), session, Vec::new())?;
                drop(server);
                loop {
                    match gameplay.service() {
                        Err(RuntimeGameplayError::Session(WorldSessionError::Io { .. })) => break,
                        Ok(_) => tokio::task::yield_now().await,
                        Err(error) => return Err(error.into()),
                    }
                }
                assert!(gameplay.active.is_none());
                Ok::<(), TestError>(())
            })
            .await?
        })
}
