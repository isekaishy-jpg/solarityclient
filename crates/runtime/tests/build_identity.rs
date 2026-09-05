//! Release-stage conversion uses the same validation as the runtime build.

#[path = "../build_support/version.rs"]
mod version;

use std::error::Error;
use std::process::Command;

use solarity_runtime::CLIENT_BUILD;

/// All four product stages retain the requested spelling outside Cargo.
#[test]
fn product_stages_have_the_requested_suffix() {
    for (cargo, product) in [
        ("0.0.0-alpha", "0.0.0a"),
        ("0.0.1-alpha", "0.0.1a"),
        ("1.2.3-beta", "1.2.3b"),
        ("1.2.3-rc", "1.2.3rc"),
        ("1.2.3", "1.2.3s"),
    ] {
        assert_eq!(version::product_version(cargo), Ok(product.to_owned()));
    }
}

/// Build metadata and invented stage names cannot silently change identity.
#[test]
fn unsupported_product_versions_are_rejected() {
    for invalid in [
        "0.0",
        "0.0.0a",
        "1.2.3-alpha.1",
        "1.2.3-stable",
        "1.2.3+4",
        "01.2.3",
    ] {
        assert!(version::product_version(invalid).is_err(), "{invalid}");
    }
}

/// Packaging must query identity without loading archives, opening a window,
/// or requiring any of the ordinary runtime configuration arguments.
#[test]
fn executable_reports_its_embedded_identity_without_startup() -> Result<(), Box<dyn Error>> {
    let executable = env!("CARGO_BIN_EXE_solarity-runtime");
    let version = Command::new(executable).arg("--version").output()?;
    assert!(version.status.success());
    assert!(version.stderr.is_empty());
    assert_eq!(
        String::from_utf8(version.stdout)?.trim(),
        CLIENT_BUILD.to_string()
    );
    let information = Command::new(executable).arg("--build-info").output()?;
    assert!(information.status.success());
    assert!(information.stderr.is_empty());
    let text = String::from_utf8(information.stdout)?;
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        [
            format!("version={}", CLIENT_BUILD.version()),
            format!("build_number={}", CLIENT_BUILD.number()),
            format!("revision={}", CLIENT_BUILD.revision()),
            format!("dirty={}", CLIENT_BUILD.is_dirty()),
        ]
    );
    Ok(())
}
