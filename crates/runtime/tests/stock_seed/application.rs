//! External tests for composition-root ownership and shutdown.

use std::error::Error;
use std::ffi::OsString;

use solarity_runtime::{ClientApplication, RuntimeConfiguration};

use crate::support::ClientFixture;

/// The composition root mounts assets and owns both executor classes.
#[test]
fn application_starts_foundations_and_shuts_down_cleanly() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let configuration = RuntimeConfiguration::from_arguments([
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--locale"),
        OsString::from("enUS"),
        OsString::from("--cpu-workers"),
        OsString::from("2"),
        OsString::from("--cpu-capacity"),
        OsString::from("8"),
        OsString::from("--network-workers"),
        OsString::from("1"),
        OsString::from("--network-shutdown-ms"),
        OsString::from("250"),
    ])?;

    let application = ClientApplication::start(configuration)?;
    let report = application.report();

    assert_eq!(report.archive_count(), 10);
    assert_eq!(report.cpu_worker_count(), 2);
    assert_eq!(report.network_worker_count(), 1);
    application.shutdown()?;
    Ok(())
}
