//! Local validation command for a real client installation.
//!
//! This example reads only caller-selected files and never copies client data
//! into the repository.

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, WdbcTable};

/// Mounts a real client archive set and reads every requested internal path.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_client_data".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let requested_paths = arguments.collect::<Vec<_>>();
    if requested_paths.is_empty() {
        return Err(usage_error(&executable).into());
    }

    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?;
    let data_root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let catalog = ArchiveCatalog::discover(data_root, Locale::from_str(locale)?)?;
    println!("discovered {} stock archives", catalog.descriptors().len());

    let mut store = AssetStore::mount(catalog)?;
    for requested_path in requested_paths {
        let requested_path = requested_path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "asset path is not UTF-8")
        })?;
        let path = AssetPath::new(requested_path)?;
        if path.as_str().ends_with(".DBC") {
            let table = WdbcTable::load(&mut store, &path)?;
            println!(
                "{} records x {} bytes\t{}\t{}",
                table.header().record_count(),
                table.header().record_size(),
                table.source().relative_path().display(),
                path
            );
            continue;
        }

        let read = store.read(&path)?;
        println!(
            "{} bytes\t{}\t{}",
            read.bytes().len(),
            read.source().relative_path().display(),
            path
        );
    }

    Ok(())
}

/// Creates one consistent usage failure for incomplete command lines.
fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <archive path> [archive path ...]",
            PathBuf::from(executable).display()
        ),
    )
}
