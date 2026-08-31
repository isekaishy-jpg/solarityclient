//! External integration tests for live advanced sound service composition.

use std::cell::Cell;
use std::error::Error;
use std::mem::size_of;
use std::num::NonZeroU16;

use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    AdvancedSoundCreateRequest, AdvancedSoundListener, AdvancedSoundService,
    AdvancedSoundServiceError, SoundCategorySettings, SoundEngine, SoundEngineSettings, SoundGain,
    SoundOutput, SoundOutputTarget, SoundResidencyPolicy, SoundVoiceState,
};

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, pcm_wav,
    sound_entries_fixture_with_advanced,
};

/// Usage-two construction starts immediately and retires after playback ends.
#[test]
fn one_shot_advanced_instance_owns_a_terminal_backend_voice() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 12_000, 0, -12_000])?;
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Pulse.wav", 1), ("", 0), ("", 0)],
        "Sound\\Advanced",
        90,
    );
    let advanced_entries = advanced_sound_entries_fixture(90, 42);
    let (mut store, _fixture) = service_fixture(
        &sound_entries,
        &advanced_entries,
        &[("Sound\\Advanced\\Pulse.wav", &wav)],
    )?;

    let _sdl_test = crate::support::sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        NonZeroU16::new(2).ok_or("voice capacity is zero")?,
        settings()?,
    )?;
    let listener = listener()?;
    let mut service = AdvancedSoundService::new();
    let random_calls = Cell::new(0);
    let mut next_word = || {
        random_calls.set(random_calls.get() + 1);
        0
    };
    let instance = service.create(
        &mut store,
        &mut engine,
        AdvancedSoundCreateRequest::new(90, Vec3::new(10.0, 0.0, 0.0), -Vec3::X),
        listener,
        &mut next_word,
    )?;

    let voice = service
        .voice(instance)
        .ok_or("one-shot instance omitted its initial voice")?;
    assert_eq!(engine.voice_state(voice)?, SoundVoiceState::Playing);
    assert_eq!(service.active_instance_count(), 1);
    // Offset and repeat interval consume words; a one-file definition returns
    // before the stock variation RNG branch.
    assert_eq!(random_calls.get(), 2);

    let mut mixed = [0_u8; 4_096];
    engine.generate(&mut mixed)?;
    assert_eq!(engine.voice_state(voice)?, SoundVoiceState::Stopped);
    assert_eq!(engine.collect_stopped_voices()?, 1);
    let report = service.update(&mut store, &mut engine, 16, 0, listener, &mut next_word)?;
    assert_eq!(report.retired, 1);
    assert_eq!(report.active, 0);
    assert_eq!(service.active_instance_count(), 0);
    assert_eq!(engine.active_voice_count(), 0);
    Ok(())
}

/// Continuous usage starts on update, swaps variation, and honors retirement.
#[test]
fn continuous_advanced_instance_drives_shared_variation_state() -> Result<(), Box<dyn Error>> {
    let wav_a = pcm_wav(8_000, &[0, 8_000, 0, -8_000])?;
    let wav_b = pcm_wav(8_000, &[0, 4_000, 0, -4_000])?;
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("First.wav", 1), ("Second.wav", 1), ("", 0)],
        "Sound\\Advanced",
        90,
    );
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    for field in 3..=6 {
        set_advanced_field(&mut advanced_entries, field, 0);
    }
    set_advanced_field(&mut advanced_entries, 8, 0);
    set_advanced_field(&mut advanced_entries, 11, 2);
    let (mut store, _fixture) = service_fixture(
        &sound_entries,
        &advanced_entries,
        &[
            ("Sound\\Advanced\\First.wav", &wav_a),
            ("Sound\\Advanced\\Second.wav", &wav_b),
        ],
    )?;

    let _sdl_test = crate::support::sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        NonZeroU16::new(2).ok_or("voice capacity is zero")?,
        settings()?,
    )?;
    let listener = listener()?;
    let mut service = AdvancedSoundService::new();
    let mut next_word = || 0;
    let instance = service.create(
        &mut store,
        &mut engine,
        AdvancedSoundCreateRequest::new(90, Vec3::new(10.0, 0.0, 0.0), -Vec3::X),
        listener,
        &mut next_word,
    )?;
    assert_eq!(engine.active_voice_count(), 0);

    let started = service.update(&mut store, &mut engine, 16, 0, listener, &mut next_word)?;
    assert_eq!(started.started, 1);
    assert_eq!(started.active, 1);
    let first_voice = service.voice(instance).ok_or("continuous voice absent")?;

    let restarted = service.update(&mut store, &mut engine, 1_001, 0, listener, &mut next_word)?;
    assert_eq!(restarted.restarted, 1);
    let second_voice = service
        .voice(instance)
        .ok_or("replacement continuous voice absent")?;
    assert_ne!(first_voice, second_voice);
    assert_eq!(engine.active_voice_count(), 1);

    service.request_retire(instance)?;
    let retired = service.update(&mut store, &mut engine, 16, 0, listener, &mut next_word)?;
    assert_eq!(retired.retired, 1);
    assert_eq!(engine.active_voice_count(), 0);
    Ok(())
}

/// Periodic instances release completed generations while their timer waits.
#[test]
fn periodic_advanced_instance_reuses_backend_after_countdown() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 6_000, 0, -6_000])?;
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Pulse.wav", 1), ("", 0), ("", 0)],
        "Sound\\Advanced",
        90,
    );
    let mut advanced_entries = advanced_sound_entries_fixture(90, 42);
    for field in 3..=6 {
        set_advanced_field(&mut advanced_entries, field, 0);
    }
    set_advanced_field(&mut advanced_entries, 8, 1);
    set_advanced_field(&mut advanced_entries, 11, 2);
    let (mut store, _fixture) = service_fixture(
        &sound_entries,
        &advanced_entries,
        &[("Sound\\Advanced\\Pulse.wav", &wav)],
    )?;
    let _sdl_test = crate::support::sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        NonZeroU16::new(1).ok_or("voice capacity is zero")?,
        settings()?,
    )?;
    let listener = listener()?;
    let mut service = AdvancedSoundService::new();
    let mut next_word = || 0;
    let instance = service.create(
        &mut store,
        &mut engine,
        AdvancedSoundCreateRequest::new(90, Vec3::new(10.0, 0.0, 0.0), -Vec3::X),
        listener,
        &mut next_word,
    )?;

    let first = service.update(&mut store, &mut engine, 1_001, 0, listener, &mut next_word)?;
    assert_eq!(first.started, 1);
    let mut mixed = [0_u8; 4_096];
    engine.generate(&mut mixed)?;

    let waiting = service.update(&mut store, &mut engine, 16, 0, listener, &mut next_word)?;
    assert_eq!(waiting.started, 0);
    assert_eq!(waiting.active, 1);
    assert_eq!(service.voice(instance), None);
    assert_eq!(engine.active_voice_count(), 0);

    let repeated = service.update(&mut store, &mut engine, 985, 0, listener, &mut next_word)?;
    assert_eq!(repeated.started, 1);
    assert!(service.voice(instance).is_some());
    assert_eq!(service.clear(&mut engine)?, 1);
    assert_eq!(service.active_instance_count(), 0);
    assert_eq!(engine.active_voice_count(), 0);
    Ok(())
}

/// Service time is monotonic and never interprets a negative runtime delta.
#[test]
fn advanced_service_rejects_negative_elapsed_time() -> Result<(), Box<dyn Error>> {
    let sound_entries =
        sound_entries_fixture_with_advanced(42, [("", 0), ("", 0), ("", 0)], "", 90);
    let advanced_entries = advanced_sound_entries_fixture(90, 42);
    let (mut store, _fixture) = service_fixture(&sound_entries, &advanced_entries, &[])?;
    let _sdl_test = crate::support::sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        NonZeroU16::new(1).ok_or("voice capacity is zero")?,
        settings()?,
    )?;
    let mut service = AdvancedSoundService::new();

    assert!(matches!(
        service.update(&mut store, &mut engine, -1, 0, listener()?, &mut || 0,),
        Err(AdvancedSoundServiceError::NegativeElapsed {
            elapsed_milliseconds: -1
        })
    ));
    Ok(())
}

/// Mounts advanced/base tables plus their exact selected payloads.
fn service_fixture<'a>(
    sound_entries: &'a [u8],
    advanced_entries: &'a [u8],
    sounds: &[(&'a str, &'a [u8])],
) -> Result<(AssetStore, Fixture), Box<dyn Error>> {
    let mut files = vec![
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: advanced_entries,
        },
    ];
    files.extend(sounds.iter().map(|(path, bytes)| FixtureFile {
        archive: "common.MPQ",
        path,
        bytes,
    }));
    let fixture = Fixture::new(&files)?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((store, fixture))
}

/// Builds the explicit CVar snapshot used by service integration tests.
fn settings() -> Result<SoundEngineSettings, Box<dyn Error>> {
    let full = SoundGain::new(1.0)?;
    Ok(SoundEngineSettings::new(
        true,
        full,
        SoundCategorySettings::new(true, full),
        SoundCategorySettings::new(true, full),
        SoundCategorySettings::new(true, full),
        SoundResidencyPolicy::new(1_048_576),
    ))
}

/// Builds the right-handed basis used by the stock world camera.
fn listener() -> Result<AdvancedSoundListener, Box<dyn Error>> {
    Ok(AdvancedSoundListener::new(
        Vec3::ZERO,
        Vec3::X,
        -Vec3::Y,
        Vec3::Z,
    )?)
}

/// Replaces one four-byte field in the fixture's first WDBC record.
fn set_advanced_field(bytes: &mut [u8], field: usize, value: u32) {
    const WDBC_HEADER_SIZE: usize = 20;
    let offset = WDBC_HEADER_SIZE + field * size_of::<u32>();
    bytes[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}
