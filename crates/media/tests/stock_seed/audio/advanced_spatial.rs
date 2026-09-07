//! External stock-compatibility tests for advanced FMOD spatial policy.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    AdvancedSoundListener, AdvancedSoundProperties, AdvancedSoundSpatialError,
    AdvancedSoundSpatialMix, SpatialSoundCatalog,
};
use solarity_rendering::WorldCamera;

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, sound_entries_fixture_with_advanced,
};

/// Character listener height follows world Z, including a pitched camera.
#[test]
fn character_listener_uses_native_back_and_up_offsets() -> Result<(), Box<dyn Error>> {
    let camera = WorldCamera::stock(
        Vec3::new(9.0, 8.0, 7.0),
        Vec3::new(1.0, 0.0, -1.0).normalize(),
        Vec3::Z,
        1000.0,
    )
    .frame(1.0)?;
    let player = Vec3::new(100.0, 200.0, 300.0);
    let listener = AdvancedSoundListener::at_character(camera, player, 2.0, 4.0)?;
    assert_eq!(
        listener.position(),
        player - camera.forward() * 2.0 + Vec3::Z * 4.0
    );
    assert!(AdvancedSoundListener::at_character(camera, player, f32::NAN, 4.0).is_err());
    Ok(())
}

/// World coordinates become SDL right/up/back coordinates at unit distance.
#[test]
fn advanced_mix_uses_camera_basis_and_stock_rolloff() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = spatial_fixture()?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let properties = AdvancedSoundProperties::from(resolved.advanced_entry());
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 1_000.0).frame(1.0)?;
    let listener = AdvancedSoundListener::from_world_camera(camera);
    let emitter = Vec3::new(10.0, -10.0, 0.0);
    let mix = AdvancedSoundSpatialMix::evaluate(
        listener,
        emitter,
        -emitter,
        properties,
        resolved.sound_entry(),
    )?;

    let coordinates = mix
        .backend_position()
        .ok_or("positioned emitter omitted its backend coordinate")?
        .coordinates();
    assert_close(coordinates[0], core::f32::consts::FRAC_1_SQRT_2);
    assert_close(coordinates[1], 0.0);
    assert_close(coordinates[2], -core::f32::consts::FRAC_1_SQRT_2);
    assert_close(mix.pan_level(), (emitter.length() - 8.0) / 88.0);
    assert_close(
        mix.distance_gain(),
        4.0 / ((emitter.length() - 4.0) * 4.0 + 4.0),
    );
    assert_close(mix.cone_gain(), 1.0);
    assert_close(
        mix.three_dimensional_gain(),
        1.0 + (mix.distance_gain() - 1.0) * mix.pan_level(),
    );
    Ok(())
}

/// Ordinary model callbacks use the base row as a fully positional voice.
#[test]
fn positioned_mix_uses_sound_entry_distance_without_advanced_policy() -> Result<(), Box<dyn Error>>
{
    let (mut store, _fixture) = spatial_fixture()?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let listener = AdvancedSoundListener::new(Vec3::ZERO, Vec3::X, -Vec3::Y, Vec3::Z)?;
    let emitter = Vec3::new(8.0, 0.0, 0.0);
    let mix =
        AdvancedSoundSpatialMix::evaluate_positioned(listener, emitter, resolved.sound_entry())?;

    assert_eq!(mix.pan_level(), 1.0);
    assert_eq!(mix.cone_gain(), 1.0);
    assert_close(mix.distance_gain(), 0.2);
    assert_close(mix.three_dimensional_gain(), 0.2);
    assert_eq!(
        mix.backend_position()
            .ok_or("positioned emitter omitted its backend coordinate")?
            .coordinates(),
        [0.0, 0.0, -1.0]
    );
    let origin =
        AdvancedSoundSpatialMix::evaluate_positioned(listener, Vec3::ZERO, resolved.sound_entry())?;
    assert_eq!(
        origin
            .backend_position()
            .ok_or("world-origin callback was changed into a 2D sound")?
            .coordinates(),
        [0.0; 3]
    );
    Ok(())
}

/// FMOD cone spreads use full angles and interpolate linearly between them.
#[test]
fn advanced_mix_applies_authored_cone_spread() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = spatial_fixture()?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let properties = AdvancedSoundProperties::from(resolved.advanced_entry());
    let listener = AdvancedSoundListener::new(Vec3::ZERO, Vec3::X, -Vec3::Y, Vec3::Z)?;
    let emitter = Vec3::new(10.0, 0.0, 0.0);

    let outside = AdvancedSoundSpatialMix::evaluate(
        listener,
        emitter,
        Vec3::X,
        properties,
        resolved.sound_entry(),
    )?;
    assert_close(outside.cone_gain(), 0.25);

    let transition = AdvancedSoundSpatialMix::evaluate(
        listener,
        emitter,
        Vec3::new(-0.5, 3.0_f32.sqrt() * 0.5, 0.0),
        properties,
        resolved.sound_entry(),
    )?;
    assert_close(transition.cone_gain(), 0.75);
    Ok(())
}

/// Stock's null emitter vector remains non-positional, not camera-relative.
#[test]
fn zero_emitter_position_keeps_the_stock_two_dimensional_path() -> Result<(), Box<dyn Error>> {
    let (mut store, _fixture) = spatial_fixture()?;
    let catalog = SpatialSoundCatalog::load(&mut store)?;
    let resolved = catalog.resolve(90)?;
    let properties = AdvancedSoundProperties::from(resolved.advanced_entry());
    let listener =
        AdvancedSoundListener::new(Vec3::new(100.0, 200.0, 300.0), Vec3::X, -Vec3::Y, Vec3::Z)?;
    let mix = AdvancedSoundSpatialMix::evaluate(
        listener,
        Vec3::ZERO,
        Vec3::X,
        properties,
        resolved.sound_entry(),
    )?;

    assert_eq!(mix.backend_position(), None);
    assert_eq!(mix.pan_level(), 0.0);
    assert_eq!(mix.three_dimensional_gain(), 1.0);
    Ok(())
}

/// Runtime listener inputs fail instead of receiving an inferred camera axis.
#[test]
fn advanced_listener_rejects_invalid_bases() {
    assert_eq!(
        AdvancedSoundListener::new(Vec3::ZERO, Vec3::X, Vec3::X, Vec3::Z),
        Err(AdvancedSoundSpatialError::ListenerBasis)
    );
    assert_eq!(
        AdvancedSoundListener::new(Vec3::NAN, Vec3::X, -Vec3::Y, Vec3::Z),
        Err(AdvancedSoundSpatialError::NonFiniteListener)
    );
}

/// Mounts the two exact build-12340 sound tables used by advanced policy.
fn spatial_fixture() -> Result<(AssetStore, Fixture), Box<dyn Error>> {
    spatial_fixture_with_range(None)
}

/// Supplies exact cutoff boundary inputs to the original-code regression.
fn spatial_fixture_with_range(
    range: Option<[u32; 2]>,
) -> Result<(AssetStore, Fixture), Box<dyn Error>> {
    let mut sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Wind.wav", 1), ("", 0), ("", 0)],
        "Sound\\Ambience",
        90,
    );
    if let Some([minimum, maximum]) = range {
        sound_entries[20 + 26 * 4..20 + 27 * 4].copy_from_slice(&minimum.to_le_bytes());
        sound_entries[20 + 27 * 4..20 + 28 * 4].copy_from_slice(&maximum.to_le_bytes());
    }
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
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((store, fixture))
}

/// Original 8782B0 output includes zero/equal ranges and adjacent cutoff floats.
#[test]
fn spatial_distance_matches_native_custom_rolloff() -> Result<(), Box<dyn Error>> {
    let listener = AdvancedSoundListener::new(Vec3::ZERO, Vec3::X, -Vec3::Y, Vec3::Z)?;
    let mut checked = 0;
    for line in include_str!("../../fixtures/sound-distance-native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let values = line
            .split_ascii_whitespace()
            .map(|value| u32::from_str_radix(value, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let (mut store, _fixture) = spatial_fixture_with_range(Some([values[0], values[1]]))?;
        let catalog = SpatialSoundCatalog::load(&mut store)?;
        let resolved = catalog.resolve(90)?;
        let mix = AdvancedSoundSpatialMix::evaluate_positioned(
            listener,
            Vec3::X * f32::from_bits(values[2]),
            resolved.sound_entry(),
        )?;
        let expected = f32::from_bits(values[3]);
        assert_eq!(mix.distance_gain().to_bits(), expected.to_bits(), "{line}");
        checked += 1;
    }
    assert_eq!(checked, 88);
    Ok(())
}

/// Compares recovered floating-point policy with a tight arithmetic tolerance.
fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
}
