//! External stock-compatibility tests for stateful sound variation selection.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, SoundEntryCatalog};
use solarity_media::{SoundVariationMode, SoundVariationSelector};

use crate::support::{Fixture, FixtureFile, sound_entries_fixture};

/// Random selection uses authored weights and excludes the previous slot.
#[test]
fn sound_variations_use_stock_shared_last_slot_state() -> Result<(), Box<dyn Error>> {
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
    let mut selector = SoundVariationSelector::new(entry).ok_or("file slots are absent")?;
    let mut words = [0, u32::MAX, 0].into_iter();
    let mut next_word = || words.next().unwrap_or_default();

    assert_eq!(
        selector
            .select(SoundVariationMode::Random, &mut next_word)
            .map(|asset| asset.path().as_str()),
        Some("SOUND\\WEIGHTED\\FIRST.WAV")
    );
    assert_eq!(selector.last_index(), Some(0));
    assert_eq!(
        selector
            .select(SoundVariationMode::Random, &mut next_word)
            .map(|asset| asset.path().as_str()),
        Some("SOUND\\WEIGHTED\\THIRD.WAV")
    );
    assert_eq!(selector.last_index(), Some(2));
    assert_eq!(
        selector
            .select(SoundVariationMode::Random, &mut next_word)
            .map(|asset| asset.path().as_str()),
        Some("SOUND\\WEIGHTED\\FIRST.WAV")
    );
    Ok(())
}

/// Sequential selection consumes no random word and activates named zero weights.
#[test]
fn sequential_variations_cycle_named_slots_without_rng() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(
        78,
        [("first.wav", 0), ("second.wav", 0), ("third.wav", 0)],
        "Sound\\Sequential",
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
    let mut selector = SoundVariationSelector::new(entry).ok_or("file slots are absent")?;
    let mut random_calls = 0;
    let mut next_word = || {
        random_calls += 1;
        u32::MAX
    };

    for expected in ["FIRST.WAV", "SECOND.WAV", "FIRST.WAV"] {
        let selected = selector
            .select(SoundVariationMode::Sequential, &mut next_word)
            .ok_or("sequential selection failed")?;
        assert!(selected.path().as_str().ends_with(expected));
    }
    assert_eq!(random_calls, 0);
    Ok(())
}

/// Random modes cannot invent weight for an all-zero authored row.
#[test]
fn zero_weight_random_sound_has_no_selection_fallback() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(
        79,
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
    let entry = catalog.entry(79).ok_or("sound entry is absent")?;
    let mut selector = SoundVariationSelector::new(entry).ok_or("file slots are absent")?;
    let mut random_calls = 0;

    assert!(
        selector
            .select(SoundVariationMode::Random, &mut || {
                random_calls += 1;
                0
            })
            .is_none()
    );
    assert_eq!(random_calls, 0);
    Ok(())
}
