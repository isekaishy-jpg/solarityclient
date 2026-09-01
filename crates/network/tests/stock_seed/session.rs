//! External stock-compatibility tests for encrypted world-session I/O.

use std::error::Error;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use solarity_network::{
    CharacterClass, CharacterGender, CharacterLoginProgress, CharacterLoginRejectionReason,
    CharacterRace, WorldAddon, WorldAddonManifest, WorldAuthProgress, WorldConnection,
    WorldObjectKind, WorldObjectUpdate,
};
use tokio::io::DuplexStream;
use wow_srp::normalized_string::NormalizedString;
use wow_srp::wrath_header::ProofSeed;
use wow_world_messages::Guid;
use wow_world_messages::wrath::opcodes::{ClientOpcodeMessage, ServerOpcodeMessage};
use wow_world_messages::wrath::{
    BillingPlanFlags, Character, Class, Expansion, Gender, Race, SMSG_AUTH_CHALLENGE,
    SMSG_AUTH_RESPONSE, SMSG_CHAR_ENUM, Vector3d,
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

        let manifest = WorldAddonManifest::new(vec![
            WorldAddon::new("Blizzard_TimeManager", true, 0x1122_3344, 0)?,
            WorldAddon::new("Solarity_Inspector", true, 0x5566_7788, 0)?,
        ])?;
        let progress = WorldConnection::authenticate(client, identity, &realm, manifest).await?;
        let mut session = match progress {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => return Err("fixture world unexpectedly queued".into()),
        };
        session.ready_for_account_data_times().await?;
        session.request_character_directory().await?;
        session.request_realm_split_info().await?;

        let addon_info = session.receive_packet().await?;
        assert_eq!(addon_info.opcode(), 0x02EF);
        assert_eq!(addon_info.name(), Some("SMSG_ADDON_INFO"));
        assert!(!addon_info.payload().is_empty());
        assert!(addon_info.character_directory()?.is_none());
        let policy = addon_info
            .addon_policy(session.addon_manifest())?
            .ok_or("add-on packet did not decode as policy")?;
        assert_eq!(policy.entries().len(), 2);
        let time_manager = policy
            .by_name("Blizzard_TimeManager")
            .ok_or("manifest name was not paired with policy")?;
        assert_eq!(time_manager.state(), 2);
        assert!(time_manager.uses_public_key());
        assert!(time_manager.crc_mismatch());
        assert_eq!(time_manager.public_key(), Some(&[0xA5; 256]));
        assert_eq!(time_manager.unknown(), Some(42));
        assert_eq!(time_manager.url(), Some("https://addons.example/keys"));
        let inspector = policy
            .by_name("Solarity_Inspector")
            .ok_or("second manifest name was not paired with policy")?;
        assert_eq!(inspector.state(), 1);
        assert!(!inspector.uses_public_key());
        assert!(!inspector.crc_mismatch());
        assert_eq!(inspector.public_key(), None);
        assert_eq!(inspector.unknown(), None);
        assert_eq!(inspector.url(), None);
        let banned = policy
            .banned_addons()
            .first()
            .ok_or("banned add-on signature was not decoded")?;
        assert_eq!(banned.id(), 7);
        assert_eq!(banned.name_md5(), [0x11; 16]);
        assert_eq!(banned.version_md5(), [0x22; 16]);
        assert_eq!(banned.timestamp(), 0x3344_5566);
        assert_eq!(banned.flags(), 0x7788_99AA);

        let large_packet = session.receive_packet().await?;
        assert_eq!(large_packet.opcode(), 0x01F5);
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

        let login = session.login_character(character).await?;
        assert_eq!(login.character_guid(), 0xF130_0000_0000_0042);
        assert_eq!(login.character_name(), "Solarion");
        assert_eq!(login.account_name(), "TESTACCOUNT");
        assert_eq!(login.realm_id(), realm.id());
        assert_eq!(login.addon_manifest().addons().len(), 2);
        let login = match login.advance().await? {
            CharacterLoginProgress::Awaiting { login, packet } => {
                assert_eq!(packet.opcode(), 0x0123);
                assert_eq!(packet.payload(), &[0x5A, 0xA5]);
                login
            }
            _ => return Err("interleaved world-entry packet was not retained".into()),
        };
        let world = match login.advance().await? {
            CharacterLoginProgress::Entered(world) => world,
            _ => return Err("login verification did not enter the world".into()),
        };
        assert_eq!(world.character_guid(), 0xF130_0000_0000_0042);
        assert_eq!(world.character_name(), "Solarion");
        assert_eq!(world.location().map_id(), 571);
        assert_eq!(world.location().x(), 5_812.25);
        assert_eq!(world.location().y(), 647.5);
        assert_eq!(world.location().z(), 647.9);
        assert_eq!(world.location().orientation(), 1.75);
        let (mut reader, mut writer) = world.split();

        let time_packet = reader.receive_packet().await?;
        assert_eq!(time_packet.name(), Some("SMSG_LOGIN_SETTIMESPEED"));
        let time = time_packet
            .world_time_speed()?
            .ok_or("world-time packet did not decode")?;
        assert_eq!(time.packed_time(), packed_realm_time());
        assert_eq!(time.year(), 2009);
        assert_eq!(time.month_index(), 11);
        assert_eq!(time.month_day(), 8);
        assert_eq!(time.weekday_index(), 2);
        assert_eq!(time.hour(), 21);
        assert_eq!(time.minute(), 37);
        assert_eq!(time.game_time_speed(), 1.0 / 60.0);
        assert_eq!(time.holiday_offset(), 0x1122_3344);

        let object_packet = reader.receive_packet().await?;
        assert_eq!(object_packet.name(), Some("SMSG_UPDATE_OBJECT"));
        let object_updates = object_packet
            .object_updates()?
            .ok_or("object packet did not decode as an update batch")?;
        assert_create_player_update(&object_updates)?;

        let compressed_packet = reader.receive_packet().await?;
        assert_eq!(
            compressed_packet.name(),
            Some("SMSG_COMPRESSED_UPDATE_OBJECT")
        );
        let compressed_updates = compressed_packet
            .object_updates()?
            .ok_or("compressed packet did not decode as an update batch")?;
        assert_eq!(compressed_updates, object_updates);

        let oversized_packet = reader.receive_packet().await?;
        let error = match oversized_packet.object_updates() {
            Err(error) => error,
            Ok(_) => return Err("oversized compressed update was accepted".into()),
        };
        assert_eq!(error.offset(), 0);
        assert_eq!(
            error.message(),
            "decompressed update exceeds the world-packet bound"
        );

        let time_sync = reader.receive_packet().await?;
        assert_eq!(time_sync.name(), Some("SMSG_TIME_SYNC_REQ"));
        assert_eq!(time_sync.time_sync_counter()?, Some(0x1122_3344));
        assert!(time_sync.pong_sequence()?.is_none());
        writer
            .send_time_sync_response(0x1122_3344, 0x5566_7788)
            .await?;
        writer.send_ping(7, 41).await?;
        let pong = reader.receive_packet().await?;
        assert_eq!(pong.name(), Some("SMSG_PONG"));
        assert_eq!(pong.pong_sequence()?, Some(7));
        assert!(pong.time_sync_counter()?.is_none());

        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// A rejected selected character restores the authenticated character-screen state.
#[test]
fn character_login_rejection_restores_character_screen() -> Result<(), Box<dyn Error + Send + Sync>>
{
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, server) = tokio::io::duplex(4_096);
        let server_task = tokio::spawn(emulate_rejected_character_login(server, session_key));
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
        let directory = packet
            .character_directory()?
            .ok_or("character directory was not returned")?;
        let character = directory
            .entries()
            .first()
            .ok_or("fixture character was not returned")?;
        let login = session.login_character(character).await?;
        match login.advance().await? {
            CharacterLoginProgress::Rejected { session, rejection } => {
                assert_eq!(rejection.result_code(), 0x54);
                assert_eq!(
                    rejection.reason(),
                    Some(CharacterLoginRejectionReason::LockedForTransfer)
                );
                assert_eq!(session.account_name(), "TESTACCOUNT");
                assert_eq!(session.realm_id(), realm.id());
            }
            _ => return Err("character login rejection did not restore selection".into()),
        }
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

/// Positional add-on policy fails at the conditional key boundary when truncated.
#[test]
fn addon_policy_rejects_truncated_public_key() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let (client, server) = tokio::io::duplex(4_096);
        let server_task = tokio::spawn(emulate_truncated_addon_policy(server, session_key));
        let manifest =
            WorldAddonManifest::new(vec![WorldAddon::new("Blizzard_TimeManager", true, 0, 0)?])?;
        let mut session =
            match WorldConnection::authenticate(client, identity, &realm, manifest).await? {
                WorldAuthProgress::Authenticated(session) => session,
                WorldAuthProgress::Queued(_) => {
                    return Err("fixture world unexpectedly queued".into());
                }
            };
        let packet = session.receive_packet().await?;
        let error = match packet.addon_policy(session.addon_manifest()) {
            Err(error) => error,
            Ok(_) => return Err("truncated add-on public key was accepted".into()),
        };
        assert_eq!(error.offset(), 3);
        assert_eq!(error.message(), "add-on public key is truncated");
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
    assert!(matches!(
        request,
        ClientOpcodeMessage::CMSG_READY_FOR_ACCOUNT_DATA_TIMES
    ));
    let request =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    assert!(matches!(request, ClientOpcodeMessage::CMSG_CHAR_ENUM));
    let request =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    let ClientOpcodeMessage::CMSG_REALM_SPLIT(request) = request else {
        return Err("fixture expected CMSG_REALM_SPLIT".into());
    };
    assert_eq!(request.realm_id, 7);

    write_encrypted_raw(&mut stream, &mut crypto, 0x02EF, &addon_policy_payload()).await?;
    write_encrypted_raw(&mut stream, &mut crypto, 0x01F5, &[0xA7; 32_768]).await?;
    let character = fixture_character();
    ServerOpcodeMessage::from(SMSG_CHAR_ENUM {
        characters: vec![character],
    })
    .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
    .await?;
    let login = ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    match login {
        ClientOpcodeMessage::CMSG_PLAYER_LOGIN(login) => {
            assert_eq!(login.guid.guid(), 0xF130_0000_0000_0042);
        }
        message => return Err(format!("unexpected character selection packet: {message}").into()),
    }
    write_encrypted_raw(&mut stream, &mut crypto, 0x0123, &[0x5A, 0xA5]).await?;
    write_encrypted_raw(&mut stream, &mut crypto, 0x0236, &world_location_payload()).await?;
    write_encrypted_raw(
        &mut stream,
        &mut crypto,
        0x0042,
        &world_time_speed_payload(),
    )
    .await?;
    write_encrypted_raw(&mut stream, &mut crypto, 0x00A9, &UPDATE_OBJECT_BODY).await?;
    write_encrypted_raw(
        &mut stream,
        &mut crypto,
        0x01F6,
        &compressed_object_update()?,
    )
    .await?;
    write_encrypted_raw(
        &mut stream,
        &mut crypto,
        0x01F6,
        &0x0080_0000_u32.to_le_bytes(),
    )
    .await?;
    write_encrypted_raw(
        &mut stream,
        &mut crypto,
        0x0390,
        &0x1122_3344_u32.to_le_bytes(),
    )
    .await?;
    let time_sync =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    match time_sync {
        ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response) => {
            assert_eq!(response.time_sync, 0x1122_3344);
            assert_eq!(response.client_ticks, 0x5566_7788);
        }
        message => return Err(format!("unexpected time-sync response: {message}").into()),
    }
    let ping = ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    match ping {
        ClientOpcodeMessage::CMSG_PING(ping) => {
            assert_eq!(ping.sequence_id, 7);
            assert_eq!(ping.round_time_in_ms, 41);
        }
        message => return Err(format!("unexpected latency probe: {message}").into()),
    }
    write_encrypted_raw(&mut stream, &mut crypto, 0x01DD, &7_u32.to_le_bytes()).await?;
    Ok(())
}

fn fixture_character() -> Character {
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
    character
}

fn addon_policy_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&[2, 1, 1]);
    payload.extend_from_slice(&[0xA5; 256]);
    payload.extend_from_slice(&42_u32.to_le_bytes());
    payload.push(1);
    payload.extend_from_slice(b"https://addons.example/keys\0");
    payload.extend_from_slice(&[1, 0, 0]);
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&7_u32.to_le_bytes());
    payload.extend_from_slice(&[0x11; 16]);
    payload.extend_from_slice(&[0x22; 16]);
    payload.extend_from_slice(&0x3344_5566_u32.to_le_bytes());
    payload.extend_from_slice(&0x7788_99AA_u32.to_le_bytes());
    payload
}

fn world_location_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(20);
    payload.extend_from_slice(&571_u32.to_le_bytes());
    payload.extend_from_slice(&5_812.25_f32.to_le_bytes());
    payload.extend_from_slice(&647.5_f32.to_le_bytes());
    payload.extend_from_slice(&647.9_f32.to_le_bytes());
    payload.extend_from_slice(&1.75_f32.to_le_bytes());
    payload
}

fn world_time_speed_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(12);
    payload.extend_from_slice(&packed_realm_time().to_le_bytes());
    payload.extend_from_slice(&(1.0_f32 / 60.0).to_le_bytes());
    payload.extend_from_slice(&0x1122_3344_u32.to_le_bytes());
    payload
}

const fn packed_realm_time() -> u32 {
    (9 << 24) | (11 << 20) | (7 << 14) | (2 << 11) | (21 << 6) | 37
}

fn assert_create_player_update(
    batch: &solarity_network::WorldObjectUpdateBatch,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let update = batch
        .updates()
        .first()
        .ok_or("fixture object update was empty")?;
    let WorldObjectUpdate::Create {
        guid,
        kind,
        second_form,
        movement,
        fields,
    } = update
    else {
        return Err("fixture object update was not CREATE_OBJECT2".into());
    };
    assert_eq!(*guid, 8);
    assert_eq!(*kind, WorldObjectKind::Player);
    assert!(*second_form);
    assert!(movement.is_self());
    assert_eq!(
        movement.position().ok_or("living position was absent")?,
        [-8_949.95, -132.493, 83.5312]
    );
    assert_eq!(movement.orientation(), Some(0.0));
    assert_eq!(movement.movement_flags(), Some(0));
    assert_eq!(
        movement
            .speeds()
            .ok_or("living speeds were absent")?
            .values(),
        [
            1.0,
            70.0,
            4.5,
            0.0,
            0.0,
            0.0,
            0.0,
            f32::from_bits(0x4049_0FD0),
            0.0,
        ]
    );
    assert_eq!(fields.len(), 6);
    assert!(fields.iter().any(|field| field.index() == 2));
    assert!(
        fields
            .iter()
            .any(|field| field.index() == 68 && field.value() == 0x4D0C)
    );
    Ok(())
}

fn compressed_object_update() -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&UPDATE_OBJECT_BODY)?;
    let compressed = encoder.finish()?;
    let mut payload = Vec::with_capacity(4 + compressed.len());
    payload.extend_from_slice(&(UPDATE_OBJECT_BODY.len() as u32).to_le_bytes());
    payload.extend_from_slice(&compressed);
    Ok(payload)
}

// Canonical build-12340 CREATE_OBJECT2 player body from the pinned protocol corpus.
const UPDATE_OBJECT_BODY: [u8; 113] = [
    0x01, 0x00, 0x00, 0x00, 0x03, 0x01, 0x08, 0x04, 0x21, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xCD, 0xD7, 0x0B, 0xC6, 0x35, 0x7E, 0x04, 0xC3, 0xF9, 0x0F, 0xA7, 0x42,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x8C, 0x42,
    0x00, 0x00, 0x90, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xD0, 0x0F, 0x49, 0x40, 0x00, 0x00, 0x00, 0x00, 0x03, 0x07, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x80, 0x00, 0x18, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x19, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0C, 0x4D, 0x00, 0x00, 0x0C, 0x4D, 0x00,
    0x00,
];

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

async fn emulate_truncated_addon_policy(
    mut stream: DuplexStream,
    session_key: [u8; 40],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut crypto = authenticate_worldserver(&mut stream, session_key).await?;
    write_encrypted_raw(&mut stream, &mut crypto, 0x02EF, &[2, 1, 1, 0xA5]).await?;
    Ok(())
}

async fn emulate_rejected_character_login(
    mut stream: DuplexStream,
    session_key: [u8; 40],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut crypto = authenticate_worldserver(&mut stream, session_key).await?;
    let request =
        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    assert!(matches!(request, ClientOpcodeMessage::CMSG_CHAR_ENUM));
    ServerOpcodeMessage::from(SMSG_CHAR_ENUM {
        characters: vec![fixture_character()],
    })
    .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
    .await?;
    let login = ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
    assert!(matches!(login, ClientOpcodeMessage::CMSG_PLAYER_LOGIN(_)));
    write_encrypted_raw(&mut stream, &mut crypto, 0x0041, &[0x54]).await?;
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
