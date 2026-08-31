//! External stock-compatibility tests for advanced sound usage lifecycles.

use std::error::Error;
use std::mem::size_of;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    AdvancedSoundDirective, AdvancedSoundLifecycle, AdvancedSoundProperties, AdvancedSoundUsage,
};

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, sound_entries_fixture_with_advanced,
};

/// Constructor randomness uses the stock multiply-high mapping and call order.
#[test]
fn advanced_lifecycle_selects_stock_instance_ranges() -> Result<(), Box<dyn Error>> {
    let properties = properties(advanced_sound_entries_fixture(90, 42))?;
    let words = [0, u32::MAX];
    let mut word_index = 0;
    let (lifecycle, directive) = AdvancedSoundLifecycle::new(properties, &mut || {
        let word = words[word_index];
        word_index += 1;
        word
    })?;

    assert_eq!(directive, AdvancedSoundDirective::Play);
    assert_eq!(lifecycle.usage(), AdvancedSoundUsage::OneShot);
    assert_eq!(lifecycle.random_offset_milliseconds(), -25);
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_999);
    assert_eq!(word_index, words.len());
    Ok(())
}

/// Equal or inverted bounds return the authored maximum without consuming RNG.
#[test]
fn advanced_lifecycle_preserves_degenerate_stock_ranges() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 7, 0);
    set_advanced_field(&mut advanced_entries, 8, 1);
    set_advanced_field(&mut advanced_entries, 9, 2_000);
    set_advanced_field(&mut advanced_entries, 10, 1_000);
    let properties = properties(advanced_entries)?;
    let mut calls = 0;
    let (lifecycle, directive) = AdvancedSoundLifecycle::new(properties, &mut || {
        calls += 1;
        0
    })?;

    assert_eq!(directive, AdvancedSoundDirective::None);
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_000);
    assert_eq!(calls, 0);
    Ok(())
}

/// Periodic playback waits until the countdown is strictly below zero.
#[test]
fn periodic_advanced_sound_uses_strict_negative_expiration() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 7, 0);
    set_advanced_field(&mut advanced_entries, 8, 1);
    let properties = properties(advanced_entries)?;
    let (mut lifecycle, directive) = AdvancedSoundLifecycle::new(properties, &mut || 0)?;

    assert_eq!(directive, AdvancedSoundDirective::None);
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_000);
    assert_eq!(
        lifecycle.update(1_000, true, false, 2, &mut || unreachable!()),
        AdvancedSoundDirective::None
    );
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 0);
    assert_eq!(
        lifecycle.update(1, true, false, 2, &mut || u32::MAX),
        AdvancedSoundDirective::Play
    );
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_999);
    Ok(())
}

/// Periodic countdowns pause outside their authored daily schedule.
#[test]
fn periodic_advanced_sound_pauses_while_schedule_is_inactive() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 7, 0);
    set_advanced_field(&mut advanced_entries, 8, 1);
    let properties = properties(advanced_entries)?;
    let (mut lifecycle, _) = AdvancedSoundLifecycle::new(properties, &mut || 0)?;

    assert_eq!(
        lifecycle.update(2_000, false, false, 2, &mut || unreachable!()),
        AdvancedSoundDirective::None
    );
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_000);
    Ok(())
}

/// Continuous sounds only vary when their resolved kit has multiple assets.
#[test]
fn continuous_advanced_sound_gates_restarts_by_variation_count() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 7, 0);
    set_advanced_field(&mut advanced_entries, 8, 0);
    let properties = properties(advanced_entries)?;
    let (mut lifecycle, directive) = AdvancedSoundLifecycle::new(properties, &mut || 0)?;

    assert_eq!(directive, AdvancedSoundDirective::None);
    assert_eq!(
        lifecycle.update(16, true, false, 2, &mut || unreachable!()),
        AdvancedSoundDirective::Play
    );
    assert_eq!(
        lifecycle.update(1_001, true, true, 1, &mut || unreachable!()),
        AdvancedSoundDirective::None
    );
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_000);
    assert_eq!(
        lifecycle.update(1_000, true, true, 2, &mut || unreachable!()),
        AdvancedSoundDirective::None
    );
    assert_eq!(
        lifecycle.update(1, true, true, 2, &mut || u32::MAX),
        AdvancedSoundDirective::Restart
    );
    assert_eq!(lifecycle.repeat_remaining_milliseconds(), 1_999);
    assert_eq!(
        lifecycle.update(16, true, false, 2, &mut || unreachable!()),
        AdvancedSoundDirective::Retire
    );
    Ok(())
}

/// A scheduled continuous sound stops once and retires after its voice ends.
#[test]
fn continuous_advanced_sound_stops_at_schedule_boundary() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 7, 0);
    set_advanced_field(&mut advanced_entries, 8, 0);
    let properties = properties(advanced_entries)?;
    let (mut lifecycle, _) = AdvancedSoundLifecycle::new(properties, &mut || 0)?;

    assert_eq!(
        lifecycle.update(16, false, true, 2, &mut || unreachable!()),
        AdvancedSoundDirective::Stop
    );
    assert_eq!(
        lifecycle.update(16, true, false, 2, &mut || unreachable!()),
        AdvancedSoundDirective::Retire
    );
    Ok(())
}

/// One-shots start in the constructor and retire after their voice completes.
#[test]
fn one_shot_advanced_sound_has_terminal_playback() -> Result<(), Box<dyn Error>> {
    let properties = properties(advanced_sound_entries_fixture(90, 42))?;
    let (mut lifecycle, directive) = AdvancedSoundLifecycle::new(properties, &mut || 0)?;

    assert_eq!(directive, AdvancedSoundDirective::Play);
    assert_eq!(
        lifecycle.update(16, false, true, 1, &mut || unreachable!()),
        AdvancedSoundDirective::None
    );
    assert_eq!(
        lifecycle.update(16, false, false, 1, &mut || unreachable!()),
        AdvancedSoundDirective::Retire
    );
    Ok(())
}

/// Unknown usage values fail rather than inheriting another mode's behavior.
#[test]
fn advanced_lifecycle_rejects_unknown_usage() -> Result<(), Box<dyn Error>> {
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    set_advanced_field(&mut advanced_entries, 8, 3);
    let properties = properties(advanced_entries)?;
    let error = match AdvancedSoundLifecycle::new(properties, &mut || unreachable!()) {
        Ok(_) => return Err("usage three unexpectedly acquired playback behavior".into()),
        Err(error) => error,
    };

    assert_eq!(error.value(), 3);
    Ok(())
}

/// Loads runtime properties through the exact advanced-to-base table fixture.
fn properties(advanced_entries: Vec<u8>) -> Result<AdvancedSoundProperties, Box<dyn Error>> {
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("First.wav", 1), ("Second.wav", 1), ("", 0)],
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
    let catalog = solarity_media::SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    Ok(AdvancedSoundProperties::from(resolved.advanced_entry()))
}

/// Replaces one four-byte field in the fixture's first WDBC record.
fn set_advanced_field(bytes: &mut [u8], field: usize, value: u32) {
    const WDBC_HEADER_SIZE: usize = 20;
    let offset = WDBC_HEADER_SIZE + field * size_of::<u32>();
    bytes[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}
