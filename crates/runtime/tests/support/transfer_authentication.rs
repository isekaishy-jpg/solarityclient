//! Synthetic SRP fixture for encrypted runtime-transfer integration tests.

use std::error::Error;
use std::net::Ipv4Addr;

use solarity_network::{
    GruntCredentials, GruntIntegrity, GruntLogin, GruntLoginOptions, LoginError, LoginLocale,
    RealmEntry, WorldIdentity,
};
use tokio::io::DuplexStream;
use wow_login_messages::Message;
use wow_login_messages::all::{Locale, Os, Platform, Population, ProtocolVersion, Version};
use wow_login_messages::version_8::opcodes::ClientOpcodeMessage;
use wow_login_messages::version_8::{
    AccountFlag, CMD_AUTH_LOGON_CHALLENGE_Server, CMD_AUTH_LOGON_CHALLENGE_Server_SecurityFlag,
    CMD_AUTH_LOGON_PROOF_Server, CMD_REALM_LIST_Server, Realm, Realm_RealmFlag,
    Realm_RealmFlag_SpecifyBuild,
};
use wow_srp::normalized_string::NormalizedString;
use wow_srp::server::SrpVerifier;
use wow_srp::{GENERATOR, LARGE_SAFE_PRIME_LITTLE_ENDIAN, PublicKey};

/// Deterministic integrity boundary for the synthetic account.
pub(super) struct TestIntegrity;

impl GruntIntegrity for TestIntegrity {
    fn proof(
        &self,
        crc_salt: [u8; 16],
        _client_public_key: [u8; 32],
    ) -> Result<[u8; 20], LoginError> {
        let mut proof = [0_u8; 20];
        proof[..16].copy_from_slice(&crc_salt);
        proof[16..].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        Ok(proof)
    }
}

/// Performs the real public login protocol and returns both cipher identities.
pub(super) async fn authenticated_identity_and_realm()
-> Result<(WorldIdentity, RealmEntry, [u8; 40]), Box<dyn Error + Send + Sync>> {
    let (client, server) = tokio::io::duplex(4_096);
    let server_task = tokio::spawn(emulate_realmd(server));
    let credentials = GruntCredentials::new("testaccount", "hunter2")?;
    let options = GruntLoginOptions::new(LoginLocale::EnUs, -240, Ipv4Addr::new(127, 0, 0, 1));
    let mut login = GruntLogin::authenticate(client, credentials, options, &TestIntegrity).await?;
    let directory = login.request_realms().await?;
    let realm = directory
        .by_id(7)
        .ok_or("realmd fixture omitted realm 7")?
        .clone();
    let identity = login.into_world_identity();
    let session_key = server_task.await??;
    Ok((identity, realm, session_key))
}

/// Serves a private in-memory realm-list exchange.
async fn emulate_realmd(
    mut stream: DuplexStream,
) -> Result<[u8; 40], Box<dyn Error + Send + Sync>> {
    let challenge = match ClientOpcodeMessage::tokio_read(&mut stream).await? {
        ClientOpcodeMessage::CMD_AUTH_LOGON_CHALLENGE(challenge) => challenge,
        message => return Err(format!("unexpected initial message: {message}").into()),
    };
    assert_eq!(challenge.protocol_version, ProtocolVersion::Eight);
    assert_eq!(
        challenge.version,
        Version {
            major: 3,
            minor: 3,
            patch: 5,
            build: 12_340
        }
    );
    assert_eq!(challenge.platform, Platform::X86);
    assert_eq!(challenge.os, Os::Windows);
    assert_eq!(challenge.locale, Locale::EnUs);
    assert_eq!(challenge.utc_timezone_offset, -240);
    assert_eq!(challenge.client_ip_address, Ipv4Addr::new(127, 0, 0, 1));
    assert_eq!(challenge.account_name, "TESTACCOUNT");

    let verifier = SrpVerifier::from_username_and_password(
        NormalizedString::new("testaccount")?,
        NormalizedString::new("hunter2")?,
    );
    let proof = verifier.into_proof();
    let crc_salt = [0xA5; 16];
    CMD_AUTH_LOGON_CHALLENGE_Server::Success {
        crc_salt,
        generator: vec![GENERATOR],
        large_safe_prime: LARGE_SAFE_PRIME_LITTLE_ENDIAN.to_vec(),
        salt: *proof.salt(),
        security_flag: CMD_AUTH_LOGON_CHALLENGE_Server_SecurityFlag::empty(),
        server_public_key: *proof.server_public_key(),
    }
    .tokio_write(&mut stream)
    .await?;

    let client_proof = match ClientOpcodeMessage::tokio_read(&mut stream).await? {
        ClientOpcodeMessage::CMD_AUTH_LOGON_PROOF(proof) => proof,
        message => return Err(format!("unexpected proof message: {message}").into()),
    };
    assert_eq!(
        client_proof.crc_hash,
        TestIntegrity.proof(crc_salt, client_proof.client_public_key)?
    );
    assert!(client_proof.telemetry_keys.is_empty());
    assert!(client_proof.security_flag.is_empty());
    let public_key = PublicKey::from_le_bytes(client_proof.client_public_key)?;
    let (server, server_proof) = proof.into_server(public_key, client_proof.client_proof)?;
    CMD_AUTH_LOGON_PROOF_Server::Success {
        account_flag: AccountFlag::new_propass(),
        hardware_survey_id: 0,
        server_proof,
        unknown: 0,
    }
    .tokio_write(&mut stream)
    .await?;

    let realm_request = ClientOpcodeMessage::tokio_read(&mut stream).await?;
    assert!(matches!(
        realm_request,
        ClientOpcodeMessage::CMD_REALM_LIST(_)
    ));
    let flag = Realm_RealmFlag::new_specify_build(Realm_RealmFlag_SpecifyBuild {
        version: Version {
            major: 3,
            minor: 3,
            patch: 5,
            build: 12_340,
        },
    })
    .set_force_green_recommended();
    CMD_REALM_LIST_Server {
        realms: vec![Realm {
            realm_type: wow_login_messages::version_2::RealmType::PlayerVsEnvironment,
            locked: false,
            flag,
            name: "Solitary Test Realm".to_owned(),
            address: "127.0.0.1:8085".to_owned(),
            population: Population::Other(1.25),
            number_of_characters_on_realm: 5,
            category: wow_login_messages::version_2::RealmCategory::One,
            realm_id: 7,
        }],
    }
    .tokio_write(&mut stream)
    .await?;
    Ok(*server.session_key())
}
