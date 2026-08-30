//! Local validation of one real build-12340 `Map.dbc` and WDT pair.

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_world_map".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let map_id = arguments.next().ok_or_else(|| usage_error(&executable))?;
    if arguments.next().is_some() {
        return Err(usage_error(&executable).into());
    }
    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?;
    let map_id = map_id
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "map ID is not UTF-8"))?
        .parse::<u32>()?;
    let root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let catalog = ArchiveCatalog::discover(root, Locale::from_str(locale)?)?;
    let mut store = AssetStore::mount(catalog)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(map_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Map.dbc does not contain map ID {map_id}"),
        )
    })?;
    let map = TerrainMap::load(&mut store, definition)?;
    println!(
        "map={} name={:?} directory={} tiles={} global_wmo={} source={}",
        map.map_id(),
        definition.name(),
        map.directory(),
        map.existing_tiles().count(),
        map.global_world_model().is_some(),
        map.source().relative_path().display(),
    );
    Ok(())
}

fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <map ID>",
            PathBuf::from(executable).display()
        ),
    )
}
