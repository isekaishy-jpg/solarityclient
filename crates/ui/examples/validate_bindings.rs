//! Validates the built-in binding vocabulary against an installed stock client.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::UiBindingCatalog;

/// Mounts the selected archive stack and validates every built-in binding body.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let data_root = ClientDataRoot::new(data_root)?;
    let archives = ArchiveCatalog::discover(data_root, locale)?;
    let archive_count = archives.descriptors().len();
    let mut store = AssetStore::mount(archives)?;
    let bindings = UiBindingCatalog::load_builtin(&mut store)?;
    let binding_count = bindings.bindings().len();
    let run_on_up_count = bindings
        .bindings()
        .filter(|binding| binding.runs_on_up())
        .count();
    let mac_only_count = bindings
        .bindings()
        .filter(|binding| binding.is_mac_only())
        .count();
    let modified_click_count = bindings.modified_clicks().count();

    println!(
        "validated {binding_count} bindings ({run_on_up_count} run on release, {mac_only_count} Mac-only) and {modified_click_count} modified clicks across {archive_count} archives"
    );
    Ok(())
}

/// Returns one stable command-line contract error.
fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_bindings <Data directory> <locale>",
    )
}
