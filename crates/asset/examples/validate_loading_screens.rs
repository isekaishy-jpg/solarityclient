//! Validates every build-12340 map-to-loading-card join against real archives.

use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, LoadingScreenCatalog,
    Locale, MapCatalog,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_loading_screens".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    if arguments.next().is_some() {
        return Err(usage_error(&executable).into());
    }
    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?;
    let root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let archives = ArchiveCatalog::discover(root, Locale::from_str(locale)?)?;
    let archive_count = archives.descriptors().len();
    let mut store = AssetStore::mount(archives)?;
    let maps = MapCatalog::load(&mut store)?;
    let screens = LoadingScreenCatalog::load(&mut store)?;

    let missing_references = maps
        .maps()
        .iter()
        .filter(|map| map.loading_screen_id() != 0)
        .filter(|map| screens.screen(map.loading_screen_id()).is_none())
        .map(|map| (map.id(), map.loading_screen_id()))
        .collect::<Vec<_>>();
    if !missing_references.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("maps reference absent loading screens: {missing_references:?}"),
        )
        .into());
    }

    let mut decoded = HashSet::<AssetPath>::new();
    let mut wide_fallbacks = Vec::new();
    for screen in screens.screens() {
        decode_once(&mut store, &mut decoded, screen.texture())?;
        if let Some(wide) = screen.widescreen_texture()? {
            if store.contains(&wide)? {
                decode_once(&mut store, &mut decoded, &wide)?;
            } else {
                wide_fallbacks.push((screen.id(), wide));
            }
        }
    }

    println!(
        "validated {} loading definitions, {} referenced maps, and {} unique BLP assets across {} archives ({} authored wide fallbacks)",
        screens.screens().len(),
        maps.maps()
            .iter()
            .filter(|map| map.loading_screen_id() != 0)
            .count(),
        decoded.len(),
        archive_count,
        wide_fallbacks.len(),
    );
    for (screen_id, path) in wide_fallbacks {
        println!("  loading_screen={screen_id} missing_optional_wide={path}");
    }
    Ok(())
}

fn decode_once(
    store: &mut AssetStore,
    decoded: &mut HashSet<AssetPath>,
    path: &AssetPath,
) -> Result<(), Box<dyn Error>> {
    if decoded.insert(path.clone()) {
        BlpTextureSource::load(store, path)?;
    }
    Ok(())
}

fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale>",
            PathBuf::from(executable).display()
        ),
    )
}
