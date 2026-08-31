//! External stock-compatibility tests for weighted sound selection.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, SoundEntryCatalog};
use solarity_media::SoundVariationSelector;

use crate::support::{Fixture, FixtureFile, sound_entries_fixture};

/// Positive frequency fields partition ticket space in authored slot order.
#[test]
fn sound_variations_use_authored_probability_weights() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(
        77,
        [("first.wav", 2), ("disabled.wav", 0), ("third.wav", 3)],
        "Sound\\Weighted",
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntries.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SoundEntryCatalog::load(&mut store)?;
    let entry = catalog.entry(77).ok_or("sound entry is absent")?;
    let selector = SoundVariationSelector::new(entry).ok_or("weighted slots are absent")?;

    assert_eq!(selector.total_weight(), 5);
    assert_eq!(
        selector.select(0).map(|asset| asset.path().as_str()),
        Some("SOUND\\WEIGHTED\\FIRST.WAV")
    );
    assert_eq!(
        selector.select(1).map(|asset| asset.path().as_str()),
        Some("SOUND\\WEIGHTED\\FIRST.WAV")
    );
    for ticket in 2..5 {
        assert_eq!(
            selector.select(ticket).map(|asset| asset.path().as_str()),
            Some("SOUND\\WEIGHTED\\THIRD.WAV")
        );
    }
    assert!(selector.select(5).is_none());
    Ok(())
}

/// A row containing only disabled slots cannot invent a playable variation.
#[test]
fn zero_weight_sound_has_no_selection_fallback() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(
        78,
        [("first.wav", 0), ("second.wav", 0), ("third.wav", 0)],
        "Sound\\Disabled",
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntries.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SoundEntryCatalog::load(&mut store)?;
    let entry = catalog.entry(78).ok_or("sound entry is absent")?;

    assert!(SoundVariationSelector::new(entry).is_none());
    Ok(())
}
