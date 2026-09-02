//! External loading-card database compatibility tests.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, LoadingScreenCatalog, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// The four build-12340 fields retain names, canonical textures, and wide flags.
#[test]
fn loading_screen_catalog_decodes_stock_layout() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, "Northrend");
    let texture = append_string(
        &mut strings,
        "Interface\\Glues\\LoadingScreens\\LoadScreenNorthrend.blp",
    );
    let table = create_wdbc(1, 4, &[7, name, texture, 1], &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\LoadingScreens.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = LoadingScreenCatalog::load(&mut store)?;
    let screen = catalog.screen(7).ok_or("loading screen is absent")?;
    assert_eq!(screen.name(), "Northrend");
    assert_eq!(
        screen.texture().as_str(),
        "INTERFACE\\GLUES\\LOADINGSCREENS\\LOADSCREENNORTHREND.BLP"
    );
    assert!(screen.has_widescreen());
    assert!(catalog.screen(8).is_none());
    Ok(())
}

/// Stock source-art names resolve to their packed BLP archive identities.
#[test]
fn loading_screen_catalog_canonicalizes_texture_references() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let first_name = append_string(&mut strings, "Extensionless");
    let first_texture = append_string(
        &mut strings,
        "Interface\\Glues\\LoadingScreens\\LoadScreenOne",
    );
    let second_name = append_string(&mut strings, "SourceTga");
    let second_texture = append_string(
        &mut strings,
        "Interface\\Glues\\LoadingScreens\\LoadScreenTwo.tga",
    );
    let table = create_wdbc(
        2,
        4,
        &[
            1,
            first_name,
            first_texture,
            0,
            2,
            second_name,
            second_texture,
            0,
        ],
        &strings,
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\LoadingScreens.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = LoadingScreenCatalog::load(&mut store)?;
    assert_eq!(
        catalog
            .screen(1)
            .ok_or("first screen is absent")?
            .texture()
            .as_str(),
        "INTERFACE\\GLUES\\LOADINGSCREENS\\LOADSCREENONE.BLP"
    );
    assert_eq!(
        catalog
            .screen(2)
            .ok_or("second screen is absent")?
            .texture()
            .as_str(),
        "INTERFACE\\GLUES\\LOADINGSCREENS\\LOADSCREENTWO.BLP"
    );
    Ok(())
}

/// Another table shape cannot be reinterpreted as build 12340.
#[test]
fn loading_screen_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 3, &[1, 0, 0], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\LoadingScreens.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        LoadingScreenCatalog::load(&mut store),
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
