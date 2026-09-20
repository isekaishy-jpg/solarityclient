//! Prints exact installed stock item rows for explicit equipped-crowd fixtures.

use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, ItemDefinitionCatalog, ItemDisplayCatalog, Locale,
};
use std::error::Error;
use std::path::PathBuf;

/// Inspects supplied item IDs; no alternate items or model paths are substituted.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let data_root = PathBuf::from(
        arguments
            .next()
            .ok_or("expected data root, locale and item ids")?,
    );
    let locale = arguments
        .next()
        .ok_or("expected locale")?
        .into_string()
        .map_err(|_| "invalid locale encoding")?
        .parse::<Locale>()?;
    let ids = arguments
        .map(|id| {
            id.into_string()
                .map_err(|_| "invalid item id encoding")
                .and_then(|id| id.parse::<u32>().map_err(|_| "invalid item id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err("expected at least one item id".into());
    }
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(data_root)?,
        locale,
    )?)?;
    let items = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    for id in ids {
        let item = items.item(id).ok_or_else(|| format!("missing item {id}"))?;
        let display = displays
            .display(item.display_info_id())
            .ok_or_else(|| format!("missing display {} for item {id}", item.display_info_id()))?;
        println!(
            "item={id} inventory={:?} display={} sheathe={} models={:?} visual={}",
            item.inventory_type(),
            item.display_info_id(),
            item.sheathe_type(),
            display.model_names(),
            display.item_visual_id()
        );
    }
    Ok(())
}
