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

/// World coordinates become SDL right/up/back coordinates at unit distance.
#[test]
fn advanced_mix_uses_camera_basis_and_fmod_inverse_rolloff() -> Result<(), Box<dyn Error>> {
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
    assert_close(mix.distance_gain(), 4.0 / emitter.length());
    assert_close(mix.cone_gain(), 1.0);
    assert_close(
        mix.three_dimensional_gain(),
        1.0 + (mix.distance_gain() - 1.0) * mix.pan_level(),
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
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((store, fixture))
}

/// Compares recovered floating-point policy with a tight arithmetic tolerance.
fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
}
