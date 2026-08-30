//! External tests for explicit startup configuration.

use std::error::Error;
use std::ffi::OsString;

use solarity_runtime::{ConfigurationError, RuntimeConfiguration};

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
    ]);
    arguments
}
