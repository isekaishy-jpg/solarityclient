//! External stock-compatibility tests for positioned build-12340 sounds.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{SpatialSoundCatalog, SpatialSoundError};

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, sound_entries_fixture_with_advanced,
};

/// MCSE resolves through the advanced row before reaching the base sound.
#[test]
fn terrain_sound_resolves_the_stock_two_step_relation() -> Result<(), Box<dyn Error>> {
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Wind.wav", 1), ("", 0), ("", 0)],
        "Sound\\Ambience",
        90,
    );
    let advanced_entries = advanced_sound_entries_fixture(90, 42);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &advanced_entries,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    assert_eq!(resolved.advanced_entry().id(), 90);
    assert_eq!(resolved.advanced_entry().sound_entry_id(), 42);
    assert_eq!(resolved.advanced_entry().outer_radius_2d(), 96.0);
    assert_eq!(resolved.sound_entry().id(), 42);
    assert_eq!(resolved.sound_entry().advanced_id(), 90);
    assert_eq!(
        resolved.sound_entry().assets()[0].path().as_str(),
        "SOUND\\AMBIENCE\\WIND.WAV"
    );
    Ok(())
}

/// Missing advanced and base rows remain distinct failures without fallback.
#[test]
fn terrain_sound_join_does_not_substitute_another_row() -> Result<(), Box<dyn Error>> {
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Wind.wav", 1), ("", 0), ("", 0)],
        "Sound\\Ambience",
        90,
    );
    let advanced_entries = advanced_sound_entries_fixture(90, 777);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &advanced_entries,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;

    assert!(matches!(
        catalog.resolve(91),
        Err(SpatialSoundError::MissingAdvancedEntry {
            advanced_entry_id: 91
        })
    ));

    assert!(matches!(
        catalog.resolve(90),
        Err(SpatialSoundError::MissingSoundEntry {
            advanced_entry_id: 90,
            sound_entry_id: 777,
        })
    ));
    Ok(())
}
