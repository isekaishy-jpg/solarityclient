//! External stock-compatibility tests for SoundEntries-driven orchestration.

use std::error::Error;
use std::num::NonZeroU16;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    SoundCategory, SoundCategorySettings, SoundDecodeMode, SoundEngine, SoundEngineError,
    SoundEngineSettings, SoundGain, SoundOutput, SoundOutputTarget, SoundPlayRequest,
    SoundPlayback,
};

use crate::support::{Fixture, FixtureFile, pcm_wav, sdl_test_lock, sound_entries_fixture};

/// Master and category CVar gains retain their evidenced zero-to-one domain.
#[test]
fn stock_sound_gain_rejects_values_outside_cvar_range() {
    assert!(SoundGain::new(0.0).is_ok());
    assert!(SoundGain::new(1.0).is_ok());
    assert!(SoundGain::new(-0.1).is_err());
    assert!(SoundGain::new(1.1).is_err());
    assert!(SoundGain::new(f32::NAN).is_err());
}

/// Live category policy mutes and restores a voice without restarting it.
#[test]
fn engine_applies_stock_volume_policy_to_active_voice() -> Result<(), Box<dyn Error>> {
    let samples = [0, 12_000, 0, -12_000].repeat(2_000);
    let wav = pcm_wav(8_000, &samples)?;
    let sound_entries = sound_entries_fixture(
        42,
        [("Pulse.wav", 2), ("Disabled.wav", 0), ("", 0)],
        "Sound\\Test",
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound\\Test\\Pulse.wav",
            bytes: &wav,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let _sdl_test = sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let enabled = settings(true)?;
    let mut engine = SoundEngine::load(&mut store, &output, capacity, enabled)?;
    let request =
        SoundPlayRequest::new(42, SoundCategory::Sfx, 0, SoundDecodeMode::Predecoded, true);
    let SoundPlayback::Started(voice) = engine.play(&mut store, request)? else {
        return Err("enabled sound was suppressed".into());
    };
    assert_eq!(engine.active_voice_count(), 1);
    assert_eq!(engine.cached_sound_count(), 1);
    assert_eq!(engine.decoded_sound_count(), 1);

    let mut audible = [0_u8; 4_096];
    assert!(engine.generate(&mut audible)? > 0);
    assert!(audible.iter().any(|byte| *byte != 0));

    engine.set_settings(settings(false)?)?;
    let mut muted = [0xFF_u8; 4_096];
    engine.generate(&mut muted)?;
    assert!(muted.iter().all(|byte| *byte == 0));

    engine.set_settings(enabled)?;
    let mut restored = [0_u8; 4_096];
    engine.generate(&mut restored)?;
    assert!(restored.iter().any(|byte| *byte != 0));
    engine.stop(voice)?;
    assert_eq!(engine.active_voice_count(), 0);
    assert!(matches!(
        engine.voice_state(voice),
        Err(SoundEngineError::UnknownVoice)
    ));
    assert_eq!(engine.collect_unused_encoded(), 1);
    Ok(())
}

/// Disabled and invalid requests never search another entry, path, or voice.
#[test]
fn engine_suppression_and_failures_have_no_fallback() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 8_000, 0, -8_000])?;
    let sound_entries =
        sound_entries_fixture(77, [("Tone.wav", 2), ("", 0), ("", 0)], "Sound\\Test");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound\\Test\\Tone.wav",
            bytes: &wav,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let _sdl_test = sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let mut engine = SoundEngine::load(&mut store, &output, capacity, settings(false)?)?;
    let missing = SoundPlayRequest::new(
        999,
        SoundCategory::Sfx,
        0,
        SoundDecodeMode::Predecoded,
        false,
    );
    assert!(matches!(
        engine.play(&mut store, missing),
        Err(SoundEngineError::MissingEntry { entry_id: 999 })
    ));

    let invalid_ticket = SoundPlayRequest::new(
        77,
        SoundCategory::Sfx,
        2,
        SoundDecodeMode::Predecoded,
        false,
    );
    assert_eq!(
        engine.play(&mut store, invalid_ticket)?,
        SoundPlayback::Suppressed
    );
    assert_eq!(engine.cached_sound_count(), 0);
    assert_eq!(engine.decoded_sound_count(), 0);

    engine.set_settings(settings(true)?)?;
    assert!(matches!(
        engine.play(&mut store, invalid_ticket),
        Err(SoundEngineError::VariationTicket {
            entry_id: 77,
            ticket: 2,
            total_weight: 2,
        })
    ));
    assert_eq!(engine.cached_sound_count(), 0);

    let valid = SoundPlayRequest::new(77, SoundCategory::Sfx, 1, SoundDecodeMode::Predecoded, true);
    let SoundPlayback::Started(voice) = engine.play(&mut store, valid)? else {
        return Err("enabled sound was suppressed".into());
    };
    assert!(matches!(
        engine.play(&mut store, valid),
        Err(SoundEngineError::Backend(
            solarity_media::SoundBackendError::VoiceCapacity
        ))
    ));
    engine.stop(voice)?;
    Ok(())
}

/// Builds explicit CVar policy without relying on media-layer defaults.
fn settings(sfx_enabled: bool) -> Result<SoundEngineSettings, Box<dyn Error>> {
    let full = SoundGain::new(1.0)?;
    Ok(SoundEngineSettings::new(
        true,
        full,
        SoundCategorySettings::new(sfx_enabled, full),
        SoundCategorySettings::new(true, SoundGain::new(0.4)?),
        SoundCategorySettings::new(true, SoundGain::new(0.6)?),
    ))
}
