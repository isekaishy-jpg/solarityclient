//! External stock-compatibility tests for game-object display records.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, GameObjectDisplayCatalog, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// The exact 19-word table retains resource identity and finite bounds.
#[test]
fn game_object_display_catalog_decodes_stock_layout() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let path = append_string(&mut strings, "World\\Generic\\Transport\\Boat.mdx");
    let mut fields = [0_u32; 19];
    fields[0] = 42;
    fields[1] = path;
    for (slot, value) in fields[12..18]
        .iter_mut()
        .zip([-3.0_f32, -2.0, -1.0, 3.0, 2.0, 1.0])
    {
        *slot = value.to_bits();
    }
    let table = create_wdbc(1, 19, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\GameObjectDisplayInfo.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = GameObjectDisplayCatalog::load(&mut store)?;
    let display = catalog.display(42).ok_or("game-object display is absent")?;
    assert_eq!(display.id(), 42);
    assert_eq!(
        display.asset_path().as_str(),
        "WORLD\\GENERIC\\TRANSPORT\\BOAT.MDX"
    );
    assert_eq!(display.minimum(), [-3.0, -2.0, -1.0]);
    assert_eq!(display.maximum(), [3.0, 2.0, 1.0]);
    assert_eq!(catalog.displays(), std::slice::from_ref(display));
    Ok(())
}

/// Another table shape cannot be reinterpreted as build 12340.
#[test]
fn game_object_display_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 18, &[0; 18], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\GameObjectDisplayInfo.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        GameObjectDisplayCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
