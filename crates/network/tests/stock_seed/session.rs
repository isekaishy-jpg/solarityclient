//! External stock-compatibility tests for encrypted world-session I/O.

use std::error::Error;

use solarity_network::{
    CharacterClass, CharacterGender, CharacterRace, WorldAddonManifest, WorldAuthProgress,
    WorldConnection,
};
use tokio::io::DuplexStream;
use wow_srp::normalized_string::NormalizedString;
use wow_srp::wrath_header::ProofSeed;
use wow_world_messages::Guid;
use wow_world_messages::wrath::opcodes::{ClientOpcodeMessage, ServerOpcodeMessage};
use wow_world_messages::wrath::{
    BillingPlanFlags, Character, Class, Expansion, Gender, Race, SMSG_ADDON_INFO,
    SMSG_AUTH_CHALLENGE, SMSG_AUTH_RESPONSE, SMSG_CHAR_ENUM, Vector3d,
};

use super::authentication::authenticated_identity_and_realm;

/// The live session retains opaque setup packets and decodes the full character row.
#[test]
fn encrypted_session_retains_addon_info_and_decodes_characters()
-> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, server) = tokio::io::duplex(8_192);
        let server_task = tokio::spawn(emulate_character_screen(server, session_key));

        let progress =
            WorldConnection::authenticate(client, identity, &realm, WorldAddonManifest::empty())
                .await?;
        let mut session = match progress {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => return Err("fixture world unexpectedly queued".into()),
        };
        session.request_character_directory().await?;

        let addon_info = session.receive_packet().await?;
        assert_eq!(addon_info.opcode(), 0x02EF);
        assert_eq!(addon_info.name(), Some("SMSG_ADDON_INFO"));
        assert!(!addon_info.payload().is_empty());
        assert!(addon_info.character_directory()?.is_none());

        let large_packet = session.receive_packet().await?;
        assert_eq!(large_packet.opcode(), 0x01F6);
        assert_eq!(large_packet.name(), None);
        assert_eq!(large_packet.payload().len(), 32_768);
        assert!(large_packet.payload().iter().all(|byte| *byte == 0xA7));
        assert!(large_packet.character_directory()?.is_none());

        let packet = session.receive_packet().await?;
        assert_eq!(packet.opcode(), 0x003B);
        assert_eq!(packet.name(), Some("SMSG_CHAR_ENUM"));
        let directory = packet
            .character_directory()?
            .ok_or("character packet did not decode as a directory")?;
        assert_eq!(directory.entries().len(), 1);
        let character = directory
            .by_guid(0xF130_0000_0000_0042)
            .ok_or("fixture character GUID was not retained")?;
        assert_eq!(directory.by_name("Solarion"), Some(character));
        assert_eq!(character.name(), "Solarion");
        assert_eq!(character.appearance().race(), CharacterRace::NightElf);
        assert_eq!(character.appearance().race().protocol_id(), 4);
        assert_eq!(character.appearance().class(), CharacterClass::Druid);
        assert_eq!(character.appearance().class().protocol_id(), 11);
        assert_eq!(character.appearance().gender(), CharacterGender::Female);
        assert_eq!(character.appearance().gender().protocol_id(), 1);
        assert_eq!(character.appearance().skin(), 2);
        assert_eq!(character.appearance().face(), 3);
        assert_eq!(character.appearance().hair_style(), 4);
        assert_eq!(character.appearance().hair_color(), 5);
        assert_eq!(character.appearance().facial_hair(), 6);
        assert_eq!(character.level(), 80);
        assert_eq!(character.location().x(), 5_812.25);
        assert_eq!(character.location().y(), 647.5);
        assert_eq!(character.location().z(), 647.9);
        assert_eq!(character.location().map_id(), 0);
        assert_eq!(character.location().area_id(), 0);
        assert_eq!(character.guild_id(), 77);
        assert_eq!(character.flags(), 0x0102_0304);
        assert_eq!(character.recustomization_flags(), 0x0506_0708);
        assert!(character.is_first_login());
        assert_eq!(character.pet().display_id(), 31_000);
        assert_eq!(character.pet().level(), 80);
        assert_eq!(character.pet().family_id(), 0);
        assert_eq!(character.equipment()[0].display_id(), 55_000);
        assert_eq!(character.equipment()[0].inventory_type_id(), 0);
        assert_eq!(character.equipment()[0].enchantment(), 3_821);

        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// Truncated character bodies fail explicitly without consuming an opaque non-character packet.
#[test]
fn character_directory_rejects_truncated_body() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, server) = tokio::io::duplex(4_096);
        let server_task = tokio::spawn(emulate_truncated_character_screen(server, session_key));
        let mut session = match WorldConnection::authenticate(
            client,
            identity,
            &realm,
            WorldAddonManifest::empty(),
        )
        .await?
        {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => return Err("fixture world unexpectedly queued".into()),
        };
        session.request_character_directory().await?;
        let packet = session.receive_packet().await?;
        let error = match packet.character_directory() {
            Err(error) => error,
            Ok(_) => return Err("truncated character body was accepted".into()),
        };
        assert!(error.offset() > 0);
        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

async fn emulate_character_screen(
    mut stream: DuplexStream,
    session_key: [u8; 40],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut crypto = authenticate_worldserver(&mut stream, session_key).await?;
    let request =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    assert!(matches!(request, ClientOpcodeMessage::CMSG_CHAR_ENUM));

    ServerOpcodeMessage::from(SMSG_ADDON_INFO { addons: Vec::new() })
        .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
        .await?;
    write_encrypted_raw(&mut stream, &mut crypto, 0x01F6, &[0xA7; 32_768]).await?;
    let mut character = Character {
        guid: Guid::new(0xF130_0000_0000_0042),
        name: "Solarion".to_owned(),
        race: Race::NightElf,
        class: Class::Druid,
        gender: Gender::Female,
        skin: 2,
        face: 3,
        hair_style: 4,
        hair_color: 5,
        facial_hair: 6,
        level: wow_world_messages::wrath::Level::new(80),
        position: Vector3d {
            x: 5_812.25,
            y: 647.5,
            z: 647.9,
        },
        guild_id: 77,
        flags: 0x0102_0304,
        recustomization_flags: 0x0506_0708,
        first_login: true,
        pet_display_id: 31_000,
        pet_level: wow_world_messages::wrath::Level::new(80),
        ..Character::default()
    };
    character.equipment[0].equipment_display_id = 55_000;
    character.equipment[0].enchantment = 3_821;
    ServerOpcodeMessage::from(SMSG_CHAR_ENUM {
        characters: vec![character],
    })
    .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
    .await?;
    Ok(())
}

async fn write_encrypted_raw(
    stream: &mut DuplexStream,
    crypto: &mut wow_srp::wrath_header::ServerCrypto,
    opcode: u16,
    payload: &[u8],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let size = u32::try_from(payload.len())? + 2;
    let header = crypto
        .encrypter()
        .encrypt_server_header(size, opcode)
        .to_vec();
    tokio::io::AsyncWriteExt::write_all(&mut *stream, &header).await?;
    tokio::io::AsyncWriteExt::write_all(&mut *stream, payload).await?;
    Ok(())
}

async fn emulate_truncated_character_screen(
    mut stream: DuplexStream,
    session_key: [u8; 40],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut crypto = authenticate_worldserver(&mut stream, session_key).await?;
    let request =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    assert!(matches!(request, ClientOpcodeMessage::CMSG_CHAR_ENUM));
    let header = crypto.encrypter().encrypt_server_header(3, 0x003B);
    tokio::io::AsyncWriteExt::write_all(&mut stream, header).await?;
    tokio::io::AsyncWriteExt::write_all(&mut stream, &[1]).await?;
    Ok(())
}

async fn authenticate_worldserver(
    stream: &mut DuplexStream,
    session_key: [u8; 40],
) -> Result<wow_srp::wrath_header::ServerCrypto, Box<dyn Error + Send + Sync>> {
    let server_seed = ProofSeed::new();
    ServerOpcodeMessage::from(SMSG_AUTH_CHALLENGE {
        unknown1: 1,
        server_seed: server_seed.seed(),
        seed: [0xC3; 32],
    })
    .tokio_write_unencrypted_server(&mut *stream)
    .await?;
    let auth_session = match ClientOpcodeMessage::tokio_read_unencrypted(&mut *stream).await? {
        ClientOpcodeMessage::CMSG_AUTH_SESSION(auth_session) => *auth_session,
        message => return Err(format!("unexpected world proof packet: {message}").into()),
    };
    let mut crypto = server_seed.into_server_header_crypto(
        &NormalizedString::new("testaccount")?,
        session_key,
        auth_session.client_proof,
        auth_session.client_seed,
    )?;
    ServerOpcodeMessage::from(SMSG_AUTH_RESPONSE::AuthOk {
        billing_flags: BillingPlanFlags::empty(),
        billing_rested: 0,
        billing_time: 0,
        expansion: Expansion::WrathOfTheLichKing,
    })
    .tokio_write_encrypted_server(&mut *stream, crypto.encrypter())
    .await?;
    Ok(crypto)
}

fn runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error + Send + Sync>> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}
