//! External stock-compatibility tests for positioned build-12340 sounds.

use std::error::Error;
use std::mem::size_of;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{AdvancedSoundProperties, SpatialSoundCatalog, SpatialSoundError};

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

/// Advanced pan blending uses full XYZ distance and the authored radius interval.
#[test]
fn advanced_sound_blends_pan_across_authored_radii() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = spatial_catalog_fixture(advanced_sound_entries_fixture(90, 42))?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let properties = AdvancedSoundProperties::from(resolved.advanced_entry());

    assert_eq!(properties.pan_level([0.0, 0.0, 0.0], [0.0, 0.0, 8.0]), 0.0);
    assert_eq!(properties.pan_level([0.0, 0.0, 0.0], [0.0, 0.0, 52.0]), 0.5);
    assert_eq!(properties.pan_level([0.0, 0.0, 0.0], [0.0, 0.0, 96.0]), 1.0);
    assert_eq!(properties.pan_level([0.0, 0.0, 0.0], [0.0, 0.0, 97.0]), 1.0);
    Ok(())
}

/// The live stock constructor corrects rollover, duck, radius, and cone ordering.
#[test]
fn advanced_sound_applies_stock_constructor_corrections() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 3, 80_000_000);
    set_advanced_field(&mut advanced_entries, 4, 10_000_000);
    set_advanced_field(&mut advanced_entries, 5, 20_000_000);
    set_advanced_field(&mut advanced_entries, 6, 30_000_000);
    set_advanced_field(&mut advanced_entries, 12, (-0.25_f32).to_bits());
    set_advanced_field(&mut advanced_entries, 13, 1.25_f32.to_bits());
    set_advanced_field(&mut advanced_entries, 15, 80.0_f32.to_bits());
    set_advanced_field(&mut advanced_entries, 16, 40.0_f32.to_bits());
    set_advanced_field(&mut advanced_entries, 19, 180.0_f32.to_bits());
    set_advanced_field(&mut advanced_entries, 20, 90.0_f32.to_bits());

    let (mut store, _fixture) = spatial_catalog_fixture(advanced_entries)?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let properties = AdvancedSoundProperties::from(resolved.advanced_entry());

    assert_eq!(
        properties.schedule_milliseconds(),
        [80_000_000, 96_400_000, 106_400_000, 116_400_000]
    );
    assert_eq!(properties.duck_gains(), [1.0, 1.0, 0.6]);
    assert_eq!(properties.influence_radii(), [40.0, 40.0]);
    assert_eq!(properties.cone_angles(), [90.0, 90.0]);
    assert_eq!(properties.outside_cone_gain(), 0.25);
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

/// Mounts the two exact sound tables needed by the spatial catalog.
fn spatial_catalog_fixture(
    advanced_entries: Vec<u8>,
) -> Result<(AssetStore, Fixture), Box<dyn Error>> {
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Wind.wav", 1), ("", 0), ("", 0)],
        "Sound\\Ambience",
        90,
    );
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
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((store, fixture))
}

/// Replaces one four-byte field in the fixture's first WDBC record.
fn set_advanced_field(bytes: &mut [u8], field: usize, value: u32) {
    const WDBC_HEADER_SIZE: usize = 20;
    let offset = WDBC_HEADER_SIZE + field * size_of::<u32>();
    bytes[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}
