//! External tests for explicit startup configuration.

use std::error::Error;
use std::ffi::OsString;

use solarity_runtime::{ConfigurationError, RuntimeConfiguration, StartupProfile, WindowMode};

use crate::support::ClientFixture;

/// Every capacity and client-selection value is explicit and typed.
#[test]
fn complete_arguments_produce_typed_configuration() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let configuration = RuntimeConfiguration::from_arguments(arguments(
        &fixture,
        &["--cpu-workers", "3", "--cpu-capacity", "24"],
    ))?;

    assert_eq!(configuration.locale().as_str(), "enUS");
    assert_eq!(
        configuration.profile_root(),
        std::fs::canonicalize(fixture.profile_root())?
    );
    assert_eq!(configuration.cpu_pool().worker_count().get(), 3);
    assert_eq!(configuration.cpu_pool().max_in_flight().get(), 24);
    assert_eq!(configuration.network_workers().get(), 2);
    assert_eq!(configuration.network_shutdown_timeout().as_millis(), 250);
    assert_eq!(
        configuration.login().endpoint().to_string(),
        "127.0.0.1:3724"
    );
    assert_eq!(
        configuration.login().options().locale(),
        solarity_network::LoginLocale::EnUs
    );
    assert_eq!(configuration.window().width(), 1280);
    assert_eq!(configuration.window().height(), 720);
    assert_eq!(configuration.window().mode(), WindowMode::Windowed);
    assert_eq!(configuration.gpu_index(), 0);
    assert!(!configuration.record_video());
    let mut recording = arguments(&fixture, &["--cpu-workers", "3", "--cpu-capacity", "24"]);
    recording.push(OsString::from("--record-video"));
    assert!(RuntimeConfiguration::from_arguments(recording.clone())?.record_video());
    recording.push(OsString::from("--record-video"));
    assert!(RuntimeConfiguration::from_arguments(recording).is_err());
    Ok(())
}

/// A fresh profile consumes the stock intro request exactly once and persists
/// that decision independently of executable replacement.
#[test]
fn intro_movie_request_is_consumed_once() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut first = StartupProfile::load(fixture.profile_root())?;
    assert!(first.play_intro_movie());
    assert!(first.consume_intro_movie()?);
    assert!(!first.play_intro_movie());

    let mut second = StartupProfile::load(fixture.profile_root())?;
    assert!(!second.play_intro_movie());
    assert!(!second.consume_intro_movie()?);
    assert_eq!(
        std::fs::read_to_string(fixture.profile_root().join("WTF/Config.wtf"))?,
        "SET playIntroMovie \"0\"\r\n"
    );
    Ok(())
}

/// Script-owned agreement changes replace existing declarations and append
/// absent ones without accumulating duplicate profile files.
#[test]
fn startup_profile_persists_changed_cvars() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    std::fs::create_dir_all(fixture.profile_root().join("WTF"))?;
    std::fs::write(
        fixture.profile_root().join("WTF/Config.wtf"),
        "SET readEULA \"-1\"\r\nSET accountName \"tester\"\r\n",
    )?;
    let mut profile = StartupProfile::load(fixture.profile_root())?;
    assert!(
        profile
            .cvar_values()
            .iter()
            .any(|(name, value)| name == "readEULA" && value == "-1")
    );

    profile.persist_cvars(&[
        ("readEULA".to_owned(), "1".to_owned()),
        ("readTOS".to_owned(), "1".to_owned()),
    ])?;

    assert_eq!(
        std::fs::read_to_string(fixture.profile_root().join("WTF/Config.wtf"))?,
        "SET readEULA \"1\"\r\nSET accountName \"tester\"\r\nSET readTOS \"1\"\r\n"
    );
    Ok(())
}

/// AccountLogin.lua reads the saved realm through GetServerName before login;
/// real realm names and Sound_OutputDriverName contain quoted whitespace.
#[test]
fn quoted_profile_values_survive_loading_and_repeated_saves() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let config = fixture.profile_root().join("WTF/Config.wtf");
    std::fs::create_dir_all(fixture.profile_root().join("WTF"))?;
    std::fs::write(
        &config,
        "\u{feff}SET realmName \"GameMap Playerbots Test\"\r\n\tset\tSound_OutputDriverName\t\"Speakers (USB Audio)\" \r\nSET accountList \"\"\r\n",
    )?;
    let mut profile = StartupProfile::load(fixture.profile_root())?;
    assert_eq!(
        profile.cvar_values(),
        &[
            ("realmName".to_owned(), "GameMap Playerbots Test".to_owned()),
            (
                "Sound_OutputDriverName".to_owned(),
                "Speakers (USB Audio)".to_owned()
            ),
            ("accountList".to_owned(), String::new()),
        ]
    );
    let updated = vec![("realmName".to_owned(), "Another Realm".to_owned())];
    profile.persist_cvars(&updated)?;
    let saved = std::fs::read_to_string(&config)?;
    profile.persist_cvars(&updated)?;
    assert_eq!(std::fs::read_to_string(&config)?, saved);
    assert_eq!(saved.matches("SET realmName ").count(), 1);
    let reopened = StartupProfile::load(fixture.profile_root())?;
    assert_eq!(reopened.cvar_values()[0], updated[0]);
    assert_eq!(reopened.cvar_values()[1].1, "Speakers (USB Audio)");
    Ok(())
}

/// Missing policy is rejected rather than replaced with a machine-dependent default.
#[test]
fn missing_required_capacity_has_no_guessed_default() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let configuration =
        RuntimeConfiguration::from_arguments(arguments(&fixture, &["--cpu-workers", "3"]));

    assert!(matches!(
        configuration,
        Err(ConfigurationError::MissingRequiredOption {
            option: "--cpu-capacity"
        })
    ));
    Ok(())
}

/// Unknown and repeated options cannot silently alter startup meaning.
#[test]
fn unknown_and_duplicate_options_are_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let unknown = RuntimeConfiguration::from_arguments(arguments(
        &fixture,
        &[
            "--cpu-workers",
            "2",
            "--cpu-capacity",
            "8",
            "--mystery",
            "1",
        ],
    ));
    assert!(matches!(
        unknown,
        Err(ConfigurationError::UnknownOption { option }) if option == "--mystery"
    ));

    let duplicate = RuntimeConfiguration::from_arguments(arguments(
        &fixture,
        &[
            "--cpu-workers",
            "2",
            "--cpu-workers",
            "4",
            "--cpu-capacity",
            "8",
        ],
    ));
    assert!(matches!(
        duplicate,
        Err(ConfigurationError::DuplicateOption {
            option: "--cpu-workers"
        })
    ));
    Ok(())
}

/// Unsupported presentation policy and invalid SDL dimensions fail explicitly.
#[test]
fn invalid_window_policy_is_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let invalid_mode = RuntimeConfiguration::from_arguments(arguments_with_window(
        &fixture,
        "1280",
        "720",
        "exclusive",
    ));
    assert!(matches!(
        invalid_mode,
        Err(ConfigurationError::InvalidWindowMode { value }) if value == "exclusive"
    ));

    let exclusive_alias = RuntimeConfiguration::from_arguments(arguments_with_window(
        &fixture,
        "1280",
        "720",
        "fullscreen",
    ));
    assert!(matches!(
        exclusive_alias,
        Err(ConfigurationError::InvalidWindowMode { value }) if value == "fullscreen"
    ));

    let invalid_width = RuntimeConfiguration::from_arguments(arguments_with_window(
        &fixture, "0", "720", "windowed",
    ));
    assert!(matches!(
        invalid_width,
        Err(ConfigurationError::InvalidWindowDimension {
            option: "--window-width",
            value
        }) if value == "0"
    ));
    Ok(())
}

/// The stock-default mode is represented explicitly as desktop composition,
/// never as an exclusive SDL display mode.
#[test]
fn fullscreen_windowed_policy_is_explicit() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let configuration = RuntimeConfiguration::from_arguments(arguments_with_window(
        &fixture,
        "1280",
        "720",
        "fullscreen-windowed",
    ))?;

    assert_eq!(
        configuration.window().mode(),
        WindowMode::FullscreenWindowed
    );
    Ok(())
}

/// Login transport identity is validated without DNS or interface inference.
#[test]
fn invalid_login_identity_is_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut invalid_endpoint = arguments(&fixture, &["--cpu-workers", "2", "--cpu-capacity", "8"]);
    replace_option_value(&mut invalid_endpoint, "--login-endpoint", "localhost")?;
    assert!(matches!(
        RuntimeConfiguration::from_arguments(invalid_endpoint),
        Err(ConfigurationError::InvalidLoginEndpoint { .. })
    ));

    let mut invalid_ip = arguments(&fixture, &["--cpu-workers", "2", "--cpu-capacity", "8"]);
    replace_option_value(&mut invalid_ip, "--login-client-ip", "localhost")?;
    assert!(matches!(
        RuntimeConfiguration::from_arguments(invalid_ip),
        Err(ConfigurationError::InvalidLoginClientIp { value }) if value == "localhost"
    ));
    Ok(())
}

/// Builds a complete argument vector with selected CPU options inserted.
fn arguments(fixture: &ClientFixture, cpu_options: &[&str]) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--profile-root"),
        fixture.profile_root().as_os_str().to_owned(),
        OsString::from("--locale"),
        OsString::from("enUS"),
    ];
    arguments.extend(cpu_options.iter().map(OsString::from));
    arguments.extend([
        OsString::from("--network-workers"),
        OsString::from("2"),
        OsString::from("--network-shutdown-ms"),
        OsString::from("250"),
        OsString::from("--login-endpoint"),
        OsString::from("127.0.0.1:3724"),
        OsString::from("--login-timezone-minutes"),
        OsString::from("-240"),
        OsString::from("--login-client-ip"),
        OsString::from("127.0.0.1"),
        OsString::from("--window-width"),
        OsString::from("1280"),
        OsString::from("--window-height"),
        OsString::from("720"),
        OsString::from("--window-mode"),
        OsString::from("windowed"),
        OsString::from("--gpu-index"),
        OsString::from("0"),
    ]);
    arguments
}

/// Builds a complete valid foundation profile with selected window policy.
fn arguments_with_window(
    fixture: &ClientFixture,
    width: &str,
    height: &str,
    mode: &str,
) -> Vec<OsString> {
    vec![
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--profile-root"),
        fixture.profile_root().as_os_str().to_owned(),
        OsString::from("--locale"),
        OsString::from("enUS"),
        OsString::from("--cpu-workers"),
        OsString::from("2"),
        OsString::from("--cpu-capacity"),
        OsString::from("8"),
        OsString::from("--network-workers"),
        OsString::from("2"),
        OsString::from("--network-shutdown-ms"),
        OsString::from("250"),
        OsString::from("--login-endpoint"),
        OsString::from("127.0.0.1:3724"),
        OsString::from("--login-timezone-minutes"),
        OsString::from("-240"),
        OsString::from("--login-client-ip"),
        OsString::from("127.0.0.1"),
        OsString::from("--window-width"),
        OsString::from(width),
        OsString::from("--window-height"),
        OsString::from(height),
        OsString::from("--window-mode"),
        OsString::from(mode),
        OsString::from("--gpu-index"),
        OsString::from("0"),
    ]
}

fn replace_option_value(
    arguments: &mut [OsString],
    option: &str,
    value: &str,
) -> Result<(), Box<dyn Error>> {
    let option_index = arguments
        .iter()
        .position(|argument| argument == option)
        .ok_or("test argument vector omitted required option")?;
    arguments[option_index + 1] = OsString::from(value);
    Ok(())
}
