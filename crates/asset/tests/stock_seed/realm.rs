//! External tests for stock realm-list client databases.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale,
    RealmCategoryCatalog, RealmConfigurationCatalog,
};

use super::support::{Fixture, FixtureFile};

/// Category rows retain masks, flags, and the exact mounted-locale name.
#[test]
fn realm_category_catalog_decodes_stock_locale_slot() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let english = append_string(&mut strings, "Oceanic");
    let french = append_string(&mut strings, "Océanique");
    let fields = category_fields(7, 0x0000_0003, 0x0000_0100, 0x20, english, french, 0x40);
    let table = create_wdbc(1, 21, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "enUS/locale-enUS.MPQ",
        path: "DBFilesClient\\Cfg_Categories.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert_eq!(store.locale(), Locale::EnUs);
    let catalog = RealmCategoryCatalog::load(&mut store)?;
    let category = catalog.category(7).ok_or("realm category is absent")?;

    assert_eq!(category.locale_mask(), 0x0000_0003);
    assert_eq!(category.character_set_mask(), 0x0000_0100);
    assert_eq!(category.flags(), 0x20);
    assert_eq!(category.name(), "Oceanic");
    assert_eq!(category.name_flags(), 0x40);
    assert_eq!(catalog.categories().len(), 1);
    assert_eq!(catalog.category(8), None);
    Ok(())
}

/// An empty selected locale remains empty instead of borrowing another language.
#[test]
fn realm_category_catalog_does_not_fallback_to_nonempty_locale() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let french = append_string(&mut strings, "Français");
    let fields = category_fields(1, 1, 0, 0, 0, french, 0);
    let table = create_wdbc(1, 21, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "enUS/locale-enUS.MPQ",
        path: "DBFilesClient\\Cfg_Categories.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = RealmCategoryCatalog::load(&mut store)?;
    assert_eq!(
        catalog.category(1).map(|category| category.name()),
        Some("")
    );
    Ok(())
}

/// Configuration rows preserve the exact rule-set values used by realm info.
#[test]
fn realm_configuration_catalog_decodes_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(2, 4, &[3, 1, 1, 0, 8, 6, 0, 1], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "enUS/locale-enUS.MPQ",
        path: "DBFilesClient\\Cfg_Configs.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = RealmConfigurationCatalog::load(&mut store)?;

    let pvp = catalog
        .configuration(3)
        .ok_or("PvP configuration is absent")?;
    assert_eq!(pvp.realm_type(), 1);
    assert!(pvp.player_killing_allowed());
    assert!(!pvp.roleplaying());
    let roleplaying = catalog
        .configuration(8)
        .ok_or("roleplaying configuration is absent")?;
    assert_eq!(roleplaying.realm_type(), 6);
    assert!(!roleplaying.player_killing_allowed());
    assert!(roleplaying.roleplaying());
    assert_eq!(catalog.configurations().len(), 2);
    Ok(())
}

/// Boolean configuration columns reject values the stock schema cannot express.
#[test]
fn realm_configuration_catalog_rejects_non_boolean_flags() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 4, &[3, 1, 2, 0], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "enUS/locale-enUS.MPQ",
        path: "DBFilesClient\\Cfg_Configs.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        RealmConfigurationCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { path, message })
            if path == AssetPath::new("DBFilesClient/Cfg_Configs.dbc")?
                && message.contains("non-Boolean value 2")
    ));
    Ok(())
}

/// Builds one physical category row with enUS and frFR locstring offsets.
fn category_fields(
    id: u32,
    locale_mask: u32,
    character_set_mask: u32,
    flags: u32,
    english: u32,
    french: u32,
    name_flags: u32,
) -> [u32; 21] {
    let mut fields = [0_u32; 21];
    fields[0] = id;
    fields[1] = locale_mask;
    fields[2] = character_set_mask;
    fields[3] = flags;
    fields[4] = english;
    fields[6] = french;
    fields[20] = name_flags;
    fields
}

/// Generates the fixed WDBC layout used by stock-era client tables.
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

/// Adds one NUL-terminated fixture string and returns its WDBC offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
