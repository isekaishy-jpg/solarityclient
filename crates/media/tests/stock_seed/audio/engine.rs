//! External stock-compatibility tests for SoundEntries-driven orchestration.

use std::cell::Cell;
use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    OwnedSoundEngine, SoundCategory, SoundCategorySettings, SoundChannel, SoundConcurrencyMode,
    SoundDecodeMode, SoundEngine, SoundEngineError, SoundEngineSettings, SoundGain, SoundLoopMode,
    SoundOutput, SoundOutputTarget, SoundPlayRequest, SoundPlayback, SoundResidencyPolicy,
    SoundSoftwareChannelCount, SoundVariationMode,
};

use crate::support::{
    Fixture, FixtureFile, advanced_sound_entries_fixture, empty_advanced_sound_entries_fixture,
    pcm_wav, sdl_test_lock, sound_entries_fixture, sound_entries_fixture_with_advanced,
    sound_entries_fixture_with_flags, ui_sound_lookups_fixture,
};

/// Master and category CVar gains retain their evidenced zero-to-one domain.
#[test]
fn stock_sound_gain_rejects_values_outside_cvar_range() {
    assert!(SoundGain::new(0.0).is_ok());
    assert!(SoundGain::new(1.0).is_ok());
    assert!(SoundGain::new(-0.1).is_err());
    assert!(SoundGain::new(1.1).is_err());
    assert!(SoundGain::new(f32::NAN).is_err());
}

/// SoundEngine.cpp selects cache residency from extension, size, and CVar clamp.
#[test]
fn stock_sound_residency_uses_exact_cacheable_size_policy() -> Result<(), Box<dyn Error>> {
    let one_megabyte = SoundResidencyPolicy::new(1_048_576, 16_777_216);
    let wav = AssetPath::new("Sound/Test.wav")?;
    let mp3 = AssetPath::new("Sound/Test.mp3")?;

    assert_eq!(
        one_megabyte.decode_mode(&wav, 1_048_576),
        SoundDecodeMode::Predecoded
    );
    assert_eq!(
        one_megabyte.decode_mode(&wav, 1_048_577),
        SoundDecodeMode::Streaming
    );
    assert_eq!(
        one_megabyte.decode_mode(&mp3, 1),
        SoundDecodeMode::Streaming
    );
    assert_eq!(
        SoundResidencyPolicy::new(u32::MAX, 0).maximum_cacheable_size_bytes(),
        2_097_152
    );
    assert_eq!(
        SoundResidencyPolicy::new(0, 0).maximum_sample_cache_size_bytes(),
        4_194_304
    );
    assert_eq!(
        SoundResidencyPolicy::new(0, 104_857_601).maximum_sample_cache_size_bytes(),
        134_217_728
    );
    Ok(())
}

/// The default request state reads bit 0x200; explicit callers can override it.
#[test]
fn stock_sound_loop_mode_uses_entry_flag_or_exact_override() {
    assert!(SoundLoopMode::Entry.is_looping(0x200));
    assert!(!SoundLoopMode::Entry.is_looping(0));
    assert!(SoundLoopMode::Loop.is_looping(0));
    assert!(!SoundLoopMode::Once.is_looping(0x200));
}

/// SoundInterface2's signed option word maps to FMOD's priority bucket.
#[test]
fn stock_sound_priority_preserves_default_and_explicit_buckets() {
    use solarity_media::SoundVoicePriority;

    assert_eq!(SoundVoicePriority::DEFAULT.value(), -1);
    assert_eq!(SoundVoicePriority::DEFAULT.effective(), 128);
    assert_eq!(SoundVoicePriority::new(0).effective(), 0);
    assert_eq!(SoundVoicePriority::new(110).effective(), 110);
    assert_eq!(SoundVoicePriority::new(256).effective(), 256);
    assert_eq!(SoundVoicePriority::new(-2).effective(), 128);
    assert_eq!(SoundVoicePriority::new(257).effective(), 128);
}

/// SoundEngine.cpp clamps the signed real-channel CVar only at initialization.
#[test]
fn stock_software_channel_count_uses_executable_clamp() {
    assert_eq!(SoundSoftwareChannelCount::new(i32::MIN).value(), 12);
    assert_eq!(SoundSoftwareChannelCount::new(11).value(), 12);
    assert_eq!(SoundSoftwareChannelCount::new(64).value(), 64);
    assert_eq!(SoundSoftwareChannelCount::new(128).value(), 128);
    assert_eq!(SoundSoftwareChannelCount::new(129).value(), 128);
    assert_eq!(SoundSoftwareChannelCount::new(i32::MAX).value(), 128);
}

/// Build 12340 resolves `PlaySound` names through UI lookup and internal-name
/// namespaces while Glue music uses only the internal-name namespace.
#[test]
fn engine_resolves_stock_script_and_glue_sound_names() -> Result<(), Box<dyn Error>> {
    let sound_entries =
        sound_entries_fixture(77, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let ui_sounds = ui_sound_lookups_fixture(&[(1, 77, "TitleOptions")]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &empty_advanced_sound_entries_fixture(),
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\UISoundLookups.dbc",
            bytes: &ui_sounds,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let _sdl_test = sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let engine = SoundEngine::load(
        &mut store,
        &output,
        SoundSoftwareChannelCount::new(1),
        settings(true)?,
    )?;

    assert_eq!(engine.script_sound_entry_id("gsTitleOptions"), Some(77));
    assert_eq!(engine.script_sound_entry_id("titleoptions"), Some(77));
    assert_eq!(engine.script_sound_entry_id("WeightedSound"), Some(77));
    assert_eq!(engine.script_sound_entry_id("77"), Some(77));
    assert_eq!(engine.internal_sound_entry_id("weightedsound"), Some(77));
    assert_eq!(engine.internal_sound_entry_id("TitleOptions"), None);
    Ok(())
}

/// Direct script sound/music paths use their exact stock channels and remain
/// independently stoppable by category.
#[test]
fn engine_plays_direct_script_paths_on_stock_categories() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, [0, 8_000, 0, -8_000].repeat(2_000).as_slice())?;
    let sound_entries =
        sound_entries_fixture(77, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &empty_advanced_sound_entries_fixture(),
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound\\Test\\Direct.wav",
            bytes: &wav,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let _sdl_test = sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        SoundSoftwareChannelCount::new(2),
        settings(true)?,
    )?;
    let path = AssetPath::new("Sound\\Test\\Direct.wav")?;

    assert!(matches!(
        engine.play_file(
            &mut store,
            &path,
            SoundChannel::SCRIPT_SOUND,
            SoundLoopMode::Once,
        )?,
        SoundPlayback::Started(_)
    ));
    assert!(matches!(
        engine.play_file(
            &mut store,
            &path,
            SoundChannel::SCRIPT_MUSIC,
            SoundLoopMode::Loop,
        )?,
        SoundPlayback::Started(_)
    ));
    assert_eq!(engine.active_voice_count(), 2);
    assert_eq!(engine.stop_category(SoundCategory::ScriptSound)?, 1);
    assert_eq!(engine.stop_category(SoundCategory::Music)?, 1);
    assert_eq!(engine.active_voice_count(), 0);
    Ok(())
}

/// Every streamed play owns one decoder object and retires it with its voice.
#[test]
fn engine_releases_noncacheable_stream_resources() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 8_000, 0, -8_000])?;
    let sound_entries =
        sound_entries_fixture(77, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &sound_entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &empty_advanced_sound_entries_fixture(),
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
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        SoundSoftwareChannelCount::new(1),
        settings_with_residency(true, 0)?,
    )?;
    let request = SoundPlayRequest::new(
        77,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Entry,
    );

    let SoundPlayback::Started(voice) = engine.play(&mut store, request, &mut || 0)? else {
        return Err("enabled stream was suppressed".into());
    };
    assert_eq!(engine.decoded_sound_count(), 1);
    let SoundPlayback::Started(second_voice) = engine.play(&mut store, request, &mut || 0)? else {
        return Err("second enabled stream was suppressed".into());
    };
    assert_eq!(engine.decoded_sound_count(), 2);
    engine.stop(second_voice)?;
    engine.stop(voice)?;
    assert_eq!(engine.decoded_sound_count(), 0);

    let once = SoundPlayRequest::new(
        77,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Once,
        SoundConcurrencyMode::Entry,
    );
    let SoundPlayback::Started(_voice) = engine.play(&mut store, once, &mut || 0)? else {
        return Err("enabled one-shot stream was suppressed".into());
    };
    let mut mixed = [0_u8; 4_096];
    engine.generate(&mut mixed)?;
    assert_eq!(engine.collect_stopped_voices()?, 1);
    assert_eq!(engine.decoded_sound_count(), 0);
    Ok(())
}

/// The process owner retains SDL's mixer until every borrowing track drops.
#[test]
fn owned_engine_contains_the_track_to_mixer_lifetime() -> Result<(), Box<dyn Error>> {
    let sound_entries =
        sound_entries_fixture(77, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let advanced_entries = empty_advanced_sound_entries_fixture();
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

    let _sdl_test = sdl_test_lock();
    let engine = OwnedSoundEngine::load(
        &mut store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(1),
        settings(true)?,
    )?;
    assert_eq!(engine.active_voice_count(), 0);
    assert_eq!(engine.settings(), settings(true)?);
    Ok(())
}

/// Advanced slider words map through the executable's eighteen channel names.
#[test]
fn advanced_volume_slider_categories_use_stock_channel_mapping() {
    assert_eq!(
        SoundChannel::new(0).map(SoundChannel::category),
        Ok(SoundCategory::Sfx)
    );
    assert_eq!(
        SoundChannel::new(5).map(SoundChannel::category),
        Ok(SoundCategory::Music)
    );
    assert_eq!(
        SoundChannel::new(2).map(SoundChannel::category),
        Ok(SoundCategory::Ambience)
    );
    assert_eq!(
        SoundChannel::new(3).map(SoundChannel::category),
        Ok(SoundCategory::Cinematic)
    );
    assert_eq!(
        SoundChannel::new(4).map(SoundChannel::category),
        Ok(SoundCategory::ScriptSound)
    );
    assert_eq!(
        SoundChannel::new(6).map(SoundChannel::category),
        Ok(SoundCategory::RacialCinematic)
    );
    for category in 7..=17 {
        assert_eq!(
            SoundChannel::new(category).map(SoundChannel::category),
            Ok(SoundCategory::Sfx)
        );
    }
    let Err(error) = SoundChannel::new(18) else {
        panic!("out-of-table category was accepted");
    };
    assert_eq!(error.value(), 18);
    let maximums = [
        None,
        None,
        None,
        None,
        None,
        None,
        Some(1),
        Some(1),
        Some(2),
        Some(1),
        Some(2),
        Some(2),
        Some(1),
        Some(6),
        Some(4),
        Some(1),
        Some(2),
        Some(4),
    ];
    for (channel, maximum) in maximums.into_iter().enumerate() {
        assert_eq!(
            SoundChannel::new(channel as u32).map(SoundChannel::maximum_active_voices),
            Ok(maximum)
        );
    }
}

/// Per-channel caps precede the base-row exclusive admission check.
#[test]
fn engine_enforces_stock_channel_caps_and_exclusivity() -> Result<(), Box<dyn Error>> {
    let samples = [0, 8_000, 0, -8_000].repeat(2_000);
    let wav = pcm_wav(8_000, &samples)?;
    let sound_entries = sound_entries_fixture_with_flags(
        77,
        [("Tone.wav", 1), ("", 0), ("", 0)],
        "Sound\\Test",
        0x20,
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
            bytes: &empty_advanced_sound_entries_fixture(),
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
    let mut engine = SoundEngine::load(
        &mut store,
        &output,
        SoundSoftwareChannelCount::new(2),
        settings(true)?,
    )?;
    let capped = SoundPlayRequest::new(
        77,
        SoundChannel::new(7)?,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Concurrent,
    );
    let SoundPlayback::Started(capped_voice) = engine.play(&mut store, capped, &mut || 0)? else {
        return Err("capped sound was suppressed".into());
    };
    assert!(matches!(
        engine.play(&mut store, capped, &mut || 0),
        Err(SoundEngineError::ChannelCapacity {
            channel: 7,
            maximum: 1
        })
    ));
    engine.stop(capped_voice)?;

    let exclusive = SoundPlayRequest::new(
        77,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Entry,
    );
    let SoundPlayback::Started(first) = engine.play(&mut store, exclusive, &mut || 0)? else {
        return Err("exclusive sound was suppressed".into());
    };
    assert!(matches!(
        engine.play(&mut store, exclusive, &mut || 0),
        Err(SoundEngineError::ExclusiveEntryActive { entry_id: 77 })
    ));
    let concurrent = SoundPlayRequest::new(
        77,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Concurrent,
    );
    let SoundPlayback::Started(second) = engine.play(&mut store, concurrent, &mut || 0)? else {
        return Err("concurrent override was suppressed".into());
    };
    engine.stop(first)?;
    engine.stop(second)?;
    Ok(())
}

/// Live category policy mutes and restores a voice without restarting it.
#[test]
fn engine_applies_stock_volume_policy_to_active_voice() -> Result<(), Box<dyn Error>> {
    let samples = [0, 12_000, 0, -12_000].repeat(2_000);
    let wav = pcm_wav(8_000, &samples)?;
    let sound_entries = sound_entries_fixture_with_advanced(
        42,
        [("Pulse.wav", 2), ("Disabled.wav", 0), ("", 0)],
        "Sound\\Test",
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
    let capacity = SoundSoftwareChannelCount::new(1);
    let enabled = settings(true)?;
    let mut engine = SoundEngine::load(&mut store, &output, capacity, enabled)?;
    let spatial = engine.resolve_spatial_sound(90)?;
    assert_eq!(spatial.advanced_entry().id(), 90);
    assert_eq!(spatial.sound_entry().id(), 42);
    let request = SoundPlayRequest::new(
        42,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Entry,
    );
    let SoundPlayback::Started(voice) = engine.play(&mut store, request, &mut || 0)? else {
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

    // Advanced schedule/spatial policy remains independent of later CVar
    // snapshots and can therefore mute without changing playback position.
    engine.set_voice_runtime_gain(voice, 0.0)?;
    engine.set_settings(settings(false)?)?;
    engine.set_settings(enabled)?;
    let mut runtime_muted = [0xFF_u8; 4_096];
    engine.generate(&mut runtime_muted)?;
    assert!(runtime_muted.iter().all(|byte| *byte == 0));
    engine.set_voice_runtime_gain(voice, 1.0)?;
    let mut runtime_restored = [0_u8; 4_096];
    engine.generate(&mut runtime_restored)?;
    assert!(runtime_restored.iter().any(|byte| *byte != 0));

    assert!(matches!(
        engine.set_voice_runtime_gain(voice, f32::NAN),
        Err(SoundEngineError::Backend(
            solarity_media::SoundBackendError::InvalidGain { .. }
        ))
    ));
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
    let advanced_entries = empty_advanced_sound_entries_fixture();
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
    let capacity = SoundSoftwareChannelCount::new(1);
    let mut engine = SoundEngine::load(&mut store, &output, capacity, settings(false)?)?;
    let missing = SoundPlayRequest::new(
        999,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Once,
        SoundConcurrencyMode::Entry,
    );
    let random_calls = Cell::new(0);
    let mut next_word = || {
        random_calls.set(random_calls.get() + 1);
        u32::MAX
    };
    assert!(matches!(
        engine.play(&mut store, missing, &mut next_word),
        Err(SoundEngineError::MissingEntry { entry_id: 999 })
    ));
    assert_eq!(random_calls.get(), 0);

    let valid = SoundPlayRequest::new(
        77,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Entry,
    );
    assert_eq!(
        engine.play(&mut store, valid, &mut next_word)?,
        SoundPlayback::Suppressed
    );
    assert_eq!(random_calls.get(), 0);
    assert_eq!(engine.cached_sound_count(), 0);
    assert_eq!(engine.decoded_sound_count(), 0);

    engine.set_settings(settings(true)?)?;
    let SoundPlayback::Started(voice) = engine.play(&mut store, valid, &mut next_word)? else {
        return Err("enabled sound was suppressed".into());
    };
    assert_eq!(random_calls.get(), 0);
    let SoundPlayback::Started(second_voice) = engine.play(&mut store, valid, &mut next_word)?
    else {
        return Err("second enabled sound was suppressed".into());
    };
    assert_eq!(random_calls.get(), 0);
    engine.stop(second_voice)?;
    engine.stop(voice)?;
    Ok(())
}

/// Builds explicit CVar policy without relying on media-layer defaults.
fn settings(sfx_enabled: bool) -> Result<SoundEngineSettings, Box<dyn Error>> {
    settings_with_residency(sfx_enabled, 1_048_576)
}

fn settings_with_residency(
    sfx_enabled: bool,
    maximum_cacheable_size_bytes: u32,
) -> Result<SoundEngineSettings, Box<dyn Error>> {
    let full = SoundGain::new(1.0)?;
    Ok(SoundEngineSettings::new(
        true,
        full,
        SoundCategorySettings::new(sfx_enabled, full),
        SoundCategorySettings::new(true, SoundGain::new(0.4)?),
        SoundCategorySettings::new(true, SoundGain::new(0.6)?),
        SoundResidencyPolicy::new(maximum_cacheable_size_bytes, 16_777_216),
    ))
}
