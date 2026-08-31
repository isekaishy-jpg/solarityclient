//! External stock-compatibility tests for liquid-query sound resolution.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{LiquidSoundCatalog, LiquidSoundError};

use crate::support::{Fixture, FixtureFile, sound_entries_fixture_with_advanced};

/// Liquid audio resolves its base sound directly without MCSE or advanced rows.
#[test]
fn liquid_sound_resolves_the_stock_direct_relation() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = liquid_catalog_fixture(42)?;
    let catalog = LiquidSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(7)?;

    assert_eq!(resolved.liquid_type().id(), 7);
    assert_eq!(resolved.liquid_type().flags(), 0x80);
    assert_eq!(resolved.liquid_type().sound_entry_id(), 42);
    assert_eq!(resolved.sound_entry().id(), 42);
    assert_eq!(resolved.sound_entry().advanced_id(), 0);
    assert_eq!(
        resolved.sound_entry().assets()[0].path().as_str(),
        "SOUND\\AMBIENCE\\RIVER.WAV"
    );
    Ok(())
}

/// Missing liquid and sound rows remain distinct failures without substitution.
#[test]
fn liquid_sound_join_has_no_mcse_or_neighbor_fallback() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = liquid_catalog_fixture(777)?;
    let catalog = LiquidSoundCatalog::load(&mut store)?;

    assert!(matches!(
        catalog.resolve(8),
        Err(LiquidSoundError::MissingLiquidType { liquid_type_id: 8 })
    ));
    assert!(matches!(
        catalog.resolve(7),
        Err(LiquidSoundError::MissingSoundEntry {
            liquid_type_id: 7,
            sound_entry_id: 777,
        })
    ));
    Ok(())
}

/// Mounts only the two exact tables used by the liquid service.
fn liquid_catalog_fixture(sound_entry_id: u32) -> Result<(AssetStore, Fixture), Box<dyn Error>> {
    let liquid_types = liquid_type_fixture(sound_entry_id);
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("River.wav", 1), ("", 0), ("", 0)],
        "Sound\\Ambience",
        0,
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LiquidType.dbc",
            bytes: &liquid_types,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
    ])?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((store, fixture))
}

/// Builds the complete 45-word row while varying only its direct sound key.
fn liquid_type_fixture(sound_entry_id: u32) -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, "River");
    let mut fields = vec![0; 45];
    fields[0] = 7;
    fields[1] = name;
    fields[2] = 0x80;
    fields[4] = sound_entry_id;
    create_wdbc(1, 45, &fields, &strings)
}

/// Serializes one fixed-field WDBC table.
fn create_wdbc(record_count: u32, field_count: u32, fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Adds one NUL-terminated fixture string and returns its table offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
