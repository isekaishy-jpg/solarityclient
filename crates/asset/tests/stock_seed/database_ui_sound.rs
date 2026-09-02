//! External stock-layout tests for interface sound lookups.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, UiSoundLookupCatalog};

use crate::support::{Fixture, FixtureFile};

/// `UISoundLookups.dbc` retains its exact key, related kit, and script name.
#[test]
fn ui_sound_lookup_loads_stock_three_word_rows() -> Result<(), Box<dyn Error>> {
    let table = ui_sound_fixture(&[(7, 42, "TitleOptions"), (3, 11, "Login")]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\UISoundLookups.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = UiSoundLookupCatalog::load(&mut store)?;
    assert_eq!(catalog.entries().len(), 2);
    assert_eq!(catalog.entries()[0].id(), 3);
    assert_eq!(
        catalog
            .entry_by_name("titleoptions")
            .map(|entry| (entry.sound_entry_id(), entry.name())),
        Some((42, "TitleOptions"))
    );
    Ok(())
}

/// Serializes the exact three-field build-12340 lookup layout.
fn ui_sound_fixture(rows: &[(u32, u32, &str)]) -> Vec<u8> {
    let mut strings = vec![0];
    let mut fields = Vec::with_capacity(rows.len() * 3);
    for (id, sound_entry_id, name) in rows {
        let offset = strings.len() as u32;
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
        fields.extend([*id, *sound_entry_id, offset]);
    }
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&12_u32.to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend(strings);
    bytes
}
