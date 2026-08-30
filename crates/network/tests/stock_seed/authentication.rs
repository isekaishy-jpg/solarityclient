//! External stock-compatibility tests for Grunt/SRP authentication.

use std::error::Error;
use std::net::Ipv4Addr;

use solarity_network::{
    GruntCredentials, GruntIntegrity, GruntLogin, GruntLoginOptions, LoginError, LoginFailure,
    LoginLocale, LoginStage, RealmCategory, RealmRecommendation, RealmType,
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

struct TestIntegrity;

impl GruntIntegrity for TestIntegrity {
    fn proof(&self, crc_salt: [u8; 16]) -> Result<[u8; 20], LoginError> {
        let mut proof = [0_u8; 20];
        proof[..16].copy_from_slice(&crc_salt);
        proof[16..].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        Ok(proof)
    }
}

/// Credentials normalize like the stock edit boxes and never expose passwords in diagnostics.
#[test]
fn credentials_normalize_once_and_redact_passwords() -> Result<(), Box<dyn Error>> {
    let credentials = GruntCredentials::new("testaccount", "hunter2")?;

    assert_eq!(credentials.username(), "TESTACCOUNT");
    let diagnostic = format!("{credentials:?}");
    assert!(diagnostic.contains("TESTACCOUNT"));
    assert!(diagnostic.contains("<redacted>"));
    assert!(!diagnostic.contains("HUNTER2"));
    assert!(matches!(
        GruntCredentials::new("seventeen-byte-id", "password"),
        Err(LoginError::InvalidUsername { .. })
    ));
    assert!(matches!(
        GruntCredentials::new("account", "pássword"),
        Err(LoginError::InvalidPassword { .. })
    ));
    Ok(())
}

/// A complete in-memory realmd exchange proves challenge bytes, mutual SRP, and realm ownership.
#[test]
fn grunt_login_authenticates_and_loads_realms() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (client, server) = tokio::io::duplex(4_096);
        let server_task = tokio::spawn(emulate_realmd(server));
        let credentials = GruntCredentials::new("testaccount", "hunter2")?;
        let options = GruntLoginOptions::new(LoginLocale::EnUs, -240, Ipv4Addr::new(127, 0, 0, 1));

        let mut login =
            GruntLogin::authenticate(client, credentials, options, &TestIntegrity).await?;
        assert_eq!(login.account_name(), "TESTACCOUNT");
        assert_eq!(login.session_key().as_bytes().len(), 40);
        assert_eq!(
            format!("{:?}", login.session_key()),
            "WorldSessionKey(<redacted>)"
        );

        let directory = login.request_realms().await?;
        assert_eq!(directory.entries().len(), 1);
        let realm = directory.by_id(7).ok_or("missing realm id 7")?;
        assert_eq!(realm.name(), "Solitary Test Realm");
        assert_eq!(realm.address(), "127.0.0.1:8085");
        assert_eq!(realm.realm_type(), RealmType::PlayerVsEnvironment);
        assert_eq!(realm.category(), RealmCategory::One);
        assert_eq!(realm.population(), 1.25);
        assert_eq!(realm.character_count(), 5);
        assert!(!realm.is_locked());
        assert!(!realm.is_invalid());
        assert!(!realm.is_offline());
        assert_eq!(realm.recommendation(), RealmRecommendation::Green);
        assert_eq!(realm.required_build(), Some((3, 3, 5, 12_340)));
        assert_eq!(directory.by_name("Solitary Test Realm"), Some(realm));

        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// A non-stock SRP group is rejected before credentials are used to calculate a proof.
#[test]
fn grunt_login_rejects_non_stock_srp_group() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (client, mut server) = tokio::io::duplex(1_024);
        let server_task = tokio::spawn(async move {
            let _challenge = ClientOpcodeMessage::tokio_read(&mut server).await?;
            let response = CMD_AUTH_LOGON_CHALLENGE_Server::Success {
                crc_salt: [0; 16],
                generator: vec![GENERATOR],
                large_safe_prime: vec![0; 32],
                salt: [0; 32],
                security_flag: CMD_AUTH_LOGON_CHALLENGE_Server_SecurityFlag::empty(),
                server_public_key: [1; 32],
            };
            response.tokio_write(&mut server).await?;
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        });
        let credentials = GruntCredentials::new("testaccount", "hunter2")?;
        let options = GruntLoginOptions::new(LoginLocale::EnUs, 0, Ipv4Addr::new(127, 0, 0, 1));

        let result = GruntLogin::authenticate(client, credentials, options, &TestIntegrity).await;

        assert!(matches!(
            result,
            Err(LoginError::InvalidSrpParameters { .. })
        ));
        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// Stock realmd result codes retain their stage and typed rejection reason.
#[test]
fn grunt_login_preserves_challenge_rejection() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let (client, mut server) = tokio::io::duplex(1_024);
        let server_task = tokio::spawn(async move {
            let _challenge = ClientOpcodeMessage::tokio_read(&mut server).await?;
            CMD_AUTH_LOGON_CHALLENGE_Server::FailBanned
                .tokio_write(&mut server)
                .await?;
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        });
        let credentials = GruntCredentials::new("testaccount", "hunter2")?;
        let options = GruntLoginOptions::new(LoginLocale::EnUs, 0, Ipv4Addr::new(127, 0, 0, 1));

        let result = GruntLogin::authenticate(client, credentials, options, &TestIntegrity).await;

        assert!(matches!(
            result,
            Err(LoginError::Rejected {
                stage: LoginStage::Challenge,
                failure: LoginFailure::Banned,
            })
        ));
        server_task.await??;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

async fn emulate_realmd(mut stream: DuplexStream) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    assert_eq!(client_proof.crc_hash, TestIntegrity.proof(crc_salt)?);
    assert!(client_proof.telemetry_keys.is_empty());
    assert!(client_proof.security_flag.is_empty());
    let public_key = PublicKey::from_le_bytes(client_proof.client_public_key)?;
    let (server, server_proof) = proof.into_server(public_key, client_proof.client_proof)?;
    CMD_AUTH_LOGON_PROOF_Server::Success {
        account_flag: AccountFlag::empty(),
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
    assert_eq!(server.session_key().len(), 40);
    Ok(())
}

fn runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error + Send + Sync>> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}
