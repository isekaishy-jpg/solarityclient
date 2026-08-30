//! Prints one UTF-8 text asset from an installed client archive stack.

use std::error::Error;
use std::io::{self, Write};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "print_client_text".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let path = arguments.next().ok_or_else(|| usage_error(&executable))?;
    if arguments.next().is_some() {
        return Err(usage_error(&executable).into());
    }

    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?
        .parse::<Locale>()?;
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "asset path is not UTF-8"))?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(PathBuf::from(data_root))?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let read = store.read(&AssetPath::new(path)?)?;
    let text = std::str::from_utf8(read.bytes())?;
    io::stdout().write_all(text.as_bytes())?;
    Ok(())
}

fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <archive text path>",
            PathBuf::from(executable).display()
        ),
    )
}
