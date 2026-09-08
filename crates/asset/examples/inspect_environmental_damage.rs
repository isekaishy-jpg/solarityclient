//! Prints the authored environmental-damage visual declarations.

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, WdbcTable};
use std::{collections::BTreeSet, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = args.next().ok_or("expected Data directory")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let table = WdbcTable::load(
        &mut store,
        &AssetPath::new("DBFilesClient/EnvironmentalDamage.dbc")?,
    )?;
    println!("EnvironmentalDamage {:?}", table.header());
    let mut kits = BTreeSet::new();
    for row in 0..table.header().record_count() {
        let fields = fields(&table, row)?;
        println!("environment {fields:?}");
        kits.insert(fields[2]);
    }
    let table = WdbcTable::load(
        &mut store,
        &AssetPath::new("DBFilesClient/SpellVisualKit.dbc")?,
    )?;
    println!("SpellVisualKit {:?}", table.header());
    let mut effects = BTreeSet::new();
    for row in 0..table.header().record_count() {
        let fields = fields(&table, row)?;
        if kits.contains(&fields[0]) {
            println!("kit {fields:?}");
            effects.extend(fields[3..15].iter().copied().filter(|id| *id != 0));
        }
    }
    let catalog = solarity_asset::SpellVisualEffectCatalog::load(&mut store)?;
    for id in effects {
        println!("effect {id} {:?}", catalog.definition(id));
    }
    let table = WdbcTable::load(
        &mut store,
        &AssetPath::new("DBFilesClient/SpellVisualKitModelAttach.dbc")?,
    )?;
    println!("SpellVisualKitModelAttach {:?}", table.header());
    for row in 0..table.header().record_count() {
        let fields = fields(&table, row)?;
        if kits.contains(&fields[1]) {
            println!("kit-attachment {fields:?}");
        }
    }
    Ok(())
}

fn fields(table: &WdbcTable, row: u32) -> Result<Vec<u32>, Box<dyn Error>> {
    Ok(table
        .record(row)
        .ok_or("record")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect())
}
