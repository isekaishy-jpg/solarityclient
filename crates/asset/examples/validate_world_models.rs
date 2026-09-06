//! Validates installed GameObject WMO roots and groups under a chosen path prefix.

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel,
    GameObjectDisplayCatalog, Locale,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: validate_world_models <Data> <locale> <archive-path-prefix>",
        )
        .into());
    }
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(&args[0])?, args[1].parse::<Locale>()?)?;
    let mut store = AssetStore::mount(catalog)?;
    let prefix = AssetPath::new(&args[2])?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let paths = displays
        .displays()
        .iter()
        .map(|display| display.asset_path())
        .filter(|path| {
            path.as_str().starts_with(prefix.as_str()) && path.as_str().ends_with(".WMO")
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    if paths.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::NotFound, "no matching GameObject WMO roots").into(),
        );
    }
    let mut failures = 0;
    for path in &paths {
        match DecodedWorldModel::load(&mut store, path) {
            Ok(model) => println!(
                "OK {path}: groups={} convex_planes={}",
                model.groups().len(),
                model.convex_volume_planes().len()
            ),
            Err(error) => {
                failures += 1;
                eprintln!("FAIL {path}: {error}");
            }
        }
    }
    println!("Checked {} WMO roots; {failures} failures", paths.len());
    if failures != 0 {
        return Err(io::Error::other("WMO validation failed").into());
    }
    Ok(())
}
