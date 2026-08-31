//! External stock-compatibility tests for advanced category ducking.

use std::error::Error;
use std::mem::size_of;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    AdvancedSoundDucking, AdvancedSoundInstanceId, AdvancedSoundProperties, SoundCategory,
    SpatialSoundCatalog,
};

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, sound_entries_fixture_with_advanced,
};

/// Stock only allocates a node when some corrected target is strictly internal.
#[test]
fn ducking_registration_requires_a_strict_internal_gain() -> Result<(), Box<dyn Error>> {
    let mut all_zero = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut all_zero, 12, 0.0_f32.to_bits());
    set_advanced_field(&mut all_zero, 13, 0.0_f32.to_bits());
    set_advanced_field(&mut all_zero, 14, 0.0_f32.to_bits());
    let mut all_one = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut all_one, 12, 1.0_f32.to_bits());
    set_advanced_field(&mut all_one, 13, 1.0_f32.to_bits());
    set_advanced_field(&mut all_one, 14, 1.0_f32.to_bits());
    let source = AdvancedSoundInstanceId::new(7);
    let mut ducking = AdvancedSoundDucking::new();

    ducking.register(source, properties(all_zero)?);
    ducking.register(source, properties(all_one)?);
    assert_eq!(ducking.influence_count(), 0);

    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);
    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);
    assert_eq!(ducking.influence_count(), 1);
    Ok(())
}

/// Duck gains move at the stock fixed unit-per-duration slope.
#[test]
fn attached_influence_ducks_each_category_toward_its_target() -> Result<(), Box<dyn Error>> {
    let source = AdvancedSoundInstanceId::new(11);
    let mut ducking = AdvancedSoundDucking::new();
    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);

    ducking.update(100, [0.0; 3], |candidate| {
        (candidate == source).then_some([8.0, 0.0, 0.0])
    });
    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.8);
    assert_eq!(ducking.category_gain(SoundCategory::Music, None), 0.8);
    assert_eq!(ducking.category_gain(SoundCategory::Ambience, None), 0.8);

    ducking.update(100, [0.0; 3], |_| Some([8.0, 0.0, 0.0]));
    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.6);
    assert_eq!(ducking.category_gain(SoundCategory::Music, None), 0.6);
    assert_eq!(ducking.category_gain(SoundCategory::Ambience, None), 0.6);
    Ok(())
}

/// The inner and outer radii form stock entry/exit hysteresis.
#[test]
fn ducking_hysteresis_detaches_at_the_strict_outer_boundary() -> Result<(), Box<dyn Error>> {
    let source = AdvancedSoundInstanceId::new(13);
    let mut ducking = AdvancedSoundDucking::new();
    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);
    ducking.update(500, [0.0; 3], |_| Some([8.0, 0.0, 0.0]));
    ducking.update(75, [0.0; 3], |_| Some([32.0, 0.0, 0.0]));
    assert!(ducking.contains_source(source));

    ducking.update(75, [0.0; 3], |_| Some([64.0, 0.0, 0.0]));
    assert!(!ducking.contains_source(source));
    assert_eq!(ducking.influence_count(), 1);
    // Stock detaches during SFX iteration; music and ambience begin recovery
    // during the remaining category iterations of the same update.
    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.2);
    assert_eq!(ducking.category_gain(SoundCategory::Music, None), 0.5);
    assert_eq!(
        ducking.category_gain(SoundCategory::Ambience, None),
        0.6_f32 + 0.1
    );

    ducking.update(600, [0.0; 3], |_| unreachable!());
    assert_eq!(ducking.influence_count(), 0);
    Ok(())
}

/// A zero emitter position uses the stock non-positional influence branch.
#[test]
fn origin_influence_ignores_listener_distance() -> Result<(), Box<dyn Error>> {
    let source = AdvancedSoundInstanceId::new(17);
    let mut ducking = AdvancedSoundDucking::new();
    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);

    ducking.update(500, [10_000.0; 3], |_| Some([0.0; 3]));
    assert!(ducking.contains_source(source));
    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.2);
    Ok(())
}

/// Aggregation takes the minimum gain and excludes the requesting source.
#[test]
fn category_gain_uses_stock_minimum_and_self_exclusion() -> Result<(), Box<dyn Error>> {
    let first = AdvancedSoundInstanceId::new(19);
    let second = AdvancedSoundInstanceId::new(23);
    let first_properties = properties_with_immediate_duck([0.2, 0.4, 0.6])?;
    let second_properties = properties_with_immediate_duck([0.1, 0.8, 0.9])?;
    let mut ducking = AdvancedSoundDucking::new();
    ducking.register(first, first_properties);
    ducking.register(second, second_properties);
    ducking.update(16, [0.0; 3], |_| Some([0.0; 3]));

    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.1);
    assert_eq!(ducking.category_gain(SoundCategory::Sfx, Some(second)), 0.2);
    assert_eq!(
        ducking.category_gain(SoundCategory::Music, Some(first)),
        0.8
    );
    Ok(())
}

/// Destroyed sources recover every category before their node is collected.
#[test]
fn removed_source_recovers_with_authored_unduck_time() -> Result<(), Box<dyn Error>> {
    let source = AdvancedSoundInstanceId::new(29);
    let mut ducking = AdvancedSoundDucking::new();
    ducking.register(source, properties(advanced_sound_entries_fixture(90, 42))?);
    ducking.update(500, [0.0; 3], |_| Some([0.0; 3]));
    ducking.remove_source(source);
    ducking.update(75, [0.0; 3], |_| unreachable!());

    assert_eq!(ducking.category_gain(SoundCategory::Sfx, None), 0.3);
    assert_eq!(ducking.category_gain(SoundCategory::Music, None), 0.5);
    assert_eq!(
        ducking.category_gain(SoundCategory::Ambience, None),
        0.6_f32 + 0.1
    );
    assert_eq!(ducking.influence_count(), 1);
    Ok(())
}

/// Produces properties with explicit gains and immediate ducking.
fn properties_with_immediate_duck(
    gains: [f32; 3],
) -> Result<AdvancedSoundProperties, Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    for (field, gain) in (12..=14).zip(gains) {
        set_advanced_field(&mut advanced_entries, field, gain.to_bits());
    }
    set_advanced_field(&mut advanced_entries, 17, 0);
    properties(advanced_entries)
}

/// Loads runtime properties through the exact advanced-to-base table fixture.
fn properties(advanced_entries: Vec<u8>) -> Result<AdvancedSoundProperties, Box<dyn Error>> {
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("First.wav", 1), ("", 0), ("", 0)],
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
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    Ok(AdvancedSoundProperties::from(
        catalog.resolve(90)?.advanced_entry(),
    ))
}

/// Replaces one four-byte field in the fixture's first WDBC record.
fn set_advanced_field(bytes: &mut [u8], field: usize, value: u32) {
    const WDBC_HEADER_SIZE: usize = 20;
    let offset = WDBC_HEADER_SIZE + field * size_of::<u32>();
    bytes[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}
