//! External tests for explicit startup configuration.

use std::error::Error;
use std::ffi::OsString;

use solarity_runtime::{ConfigurationError, RuntimeConfiguration, WindowMode};

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
    assert_eq!(configuration.cpu_pool().worker_count().get(), 3);
    assert_eq!(configuration.cpu_pool().max_in_flight().get(), 24);
    assert_eq!(configuration.network_workers().get(), 2);
    assert_eq!(configuration.network_shutdown_timeout().as_millis(), 250);
    assert_eq!(configuration.window().width(), 1280);
    assert_eq!(configuration.window().height(), 720);
    assert_eq!(configuration.window().mode(), WindowMode::Windowed);
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

/// Builds a complete argument vector with selected CPU options inserted.
fn arguments(fixture: &ClientFixture, cpu_options: &[&str]) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--locale"),
        OsString::from("enUS"),
    ];
    arguments.extend(cpu_options.iter().map(OsString::from));
    arguments.extend([
        OsString::from("--network-workers"),
        OsString::from("2"),
        OsString::from("--network-shutdown-ms"),
        OsString::from("250"),
        OsString::from("--window-width"),
        OsString::from("1280"),
        OsString::from("--window-height"),
        OsString::from("720"),
        OsString::from("--window-mode"),
        OsString::from("windowed"),
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
        OsString::from("--window-width"),
        OsString::from(width),
        OsString::from("--window-height"),
        OsString::from(height),
        OsString::from("--window-mode"),
        OsString::from(mode),
    ]
}
