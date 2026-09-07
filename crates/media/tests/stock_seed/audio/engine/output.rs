//! Requested mixer formats and native restart voice/worker retirement.

use super::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Error, Fixture, FixtureFile, Locale,
    OwnedSoundEngine, SoundChannel, SoundEngineError, SoundLoopMode, SoundOutput,
    SoundOutputTarget, SoundPlayback, SoundSoftwareChannelCount,
    empty_advanced_sound_entries_fixture, pcm_wav, sdl_test_lock, settings, sound_entries_fixture,
};
use crate::support::sound_entries_fixture_with_flags;
use solarity_media::{
    SoundCache, SoundConcurrencyMode, SoundOutputConfiguration, SoundOutputQuality,
    SoundPlayRequest, SoundVariationMode, SoundVoiceState,
};

/// Authored pitch changes playback duration and is cleared on direct-file reuse.
#[test]
fn selected_pitch_reaches_mixer_and_does_not_leak_into_reused_tracks() -> Result<(), Box<dyn Error>>
{
    let entries = sound_entries_fixture_with_flags(
        77,
        [("Tone.wav", 1), ("", 0), ("", 0)],
        "Sound/Test",
        0x400,
    );
    let wav = pcm_wav(8_000, &[0, 8_000, 0, -8_000].repeat(2_000))?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/SoundEntries.dbc",
            bytes: &entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/SoundEntriesAdvanced.dbc",
            bytes: &empty_advanced_sound_entries_fixture(),
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound/Test/Tone.wav",
            bytes: &wav,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let _sdl = sdl_test_lock();
    let mut owner = OwnedSoundEngine::load(
        &mut store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(12),
        settings(true)?,
    )?;
    owner.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        let request = SoundPlayRequest::new(
            77,
            SoundChannel::SFX,
            SoundVariationMode::Random,
            SoundLoopMode::Once,
            SoundConcurrencyMode::Concurrent,
        );
        let bytes_per_second = engine.output_info().sample_rate_hz() as usize * 4;
        let mut block = vec![0; bytes_per_second * 93 / 100 / 4 * 4];
        for (word, expected) in [
            (0, SoundVoiceState::Playing),
            (0x7fffff, SoundVoiceState::Stopped),
        ] {
            let SoundPlayback::Started(voice) = engine.play(&mut store, request, &mut || word)?
            else {
                return Err("pitch test voice was suppressed".into());
            };
            engine.generate(&mut block)?;
            assert!(block.iter().any(|byte| *byte != 0));
            assert_eq!(engine.voice_state(voice)?, expected);
            engine.stop(voice)?;
        }
        let SoundPlayback::Started(voice) = engine.play_file(
            &mut store,
            &AssetPath::new("Sound/Test/Tone.wav")?,
            SoundChannel::SCRIPT_SOUND,
            SoundLoopMode::Once,
        )?
        else {
            return Err("direct test voice was suppressed".into());
        };
        engine.generate(&mut block)?;
        assert_eq!(engine.voice_state(voice)?, SoundVoiceState::Playing);
        engine.generate(&mut block)?;
        assert_eq!(engine.voice_state(voice)?, SoundVoiceState::Stopped);
        Ok(())
    })?;
    Ok(())
}

/// 87C710 requests 22.05/44.1/48 kHz for the three stock menu values.
#[test]
fn output_quality_selects_native_sample_rates() -> Result<(), Box<dyn Error>> {
    let _sdl_test = sdl_test_lock();
    for (quality, sample_rate) in [
        (SoundOutputQuality::Low, 22_050),
        (SoundOutputQuality::Medium, 44_100),
        (SoundOutputQuality::High, 48_000),
    ] {
        let output = SoundOutput::open_configured(SoundOutputConfiguration {
            target: SoundOutputTarget::Memory,
            quality,
        })?;
        assert_eq!(output.info().sample_rate_hz(), sample_rate);
        assert_eq!(output.info().channel_count(), 2);
    }
    Ok(())
}

/// 87DED0 cancels loads and stops old tracks; 87B490 preserves logical handles
/// until their owners observe retirement, even while a new device plays audio.
#[test]
fn output_restart_retires_voices_and_pending_loads_before_reopening() -> Result<(), Box<dyn Error>>
{
    let entries = sound_entries_fixture(77, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let advanced = empty_advanced_sound_entries_fixture();
    let wav = pcm_wav(8_000, &[0, 8_000, 0, -8_000].repeat(2_000))?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &entries,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &advanced,
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
    let mut owner = OwnedSoundEngine::load(
        &mut store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(64),
        settings(true)?,
    )?;
    let path = AssetPath::new("Sound\\Test\\Tone.wav")?;
    let SoundPlayback::Started(old_voice) = owner.with_engine_mut(|engine| {
        engine.play_file(
            &mut store,
            &path,
            SoundChannel::SCRIPT_MUSIC,
            SoundLoopMode::Loop,
        )
    })?
    else {
        return Err("initial sound was suppressed".into());
    };
    let pending = owner
        .with_engine_mut(|engine| {
            engine.begin_file_load(&path, SoundChannel::SCRIPT_SOUND, SoundLoopMode::Once)
        })?
        .ok_or("missing pending load")?
        .handle();
    let mut mixed = [0_u8; 4_096];
    let late_payload = SoundCache::new().load(&mut store, &path)?;
    owner.with_engine_mut(|engine| engine.generate(&mut mixed))?;
    assert!(mixed.iter().any(|sample| *sample != 0));

    owner.with_engine_mut(|engine| engine.set_background_muted(true))?;
    owner.restart(
        SoundOutputConfiguration {
            target: SoundOutputTarget::Memory,
            quality: SoundOutputQuality::High,
        },
        SoundSoftwareChannelCount::new(1),
    )?;
    owner.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        assert_eq!(engine.output_info().sample_rate_hz(), 48_000);
        assert_eq!(engine.software_channel_count(), 12);
        assert_eq!(engine.voice_state(old_voice)?, SoundVoiceState::Stopped);
        assert!(!engine.is_load_pending(pending));
        assert_eq!(
            engine.complete_load(pending, &late_payload)?,
            SoundPlayback::Suppressed
        );
        engine.generate(&mut mixed)?;
        assert!(mixed.iter().all(|sample| *sample == 0));
        assert_eq!(engine.collect_stopped_voices()?, 1);
        assert!(matches!(
            engine.voice_state(old_voice),
            Err(SoundEngineError::UnknownVoice)
        ));
        Ok(())
    })?;
    let SoundPlayback::Started(new_voice) = owner.with_engine_mut(|engine| {
        engine.play_file(
            &mut store,
            &path,
            SoundChannel::SCRIPT_MUSIC,
            SoundLoopMode::Loop,
        )
    })?
    else {
        return Err("replacement sound was suppressed".into());
    };
    assert_ne!(new_voice, old_voice);
    owner.with_engine_mut(|engine| engine.generate(&mut mixed))?;
    assert!(mixed.iter().all(|sample| *sample == 0));
    owner.with_engine_mut(|engine| engine.set_background_muted(false))?;
    owner.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        engine.set_background_muted(true)?;
        engine.generate(&mut mixed)?;
        assert!(mixed.iter().all(|sample| *sample == 0));
        assert_eq!(engine.voice_state(new_voice)?, SoundVoiceState::Playing);
        engine.set_background_muted(false)?;
        engine.generate(&mut mixed)?;
        assert!(mixed.iter().any(|sample| *sample != 0));
        assert_eq!(engine.voice_state(new_voice)?, SoundVoiceState::Playing);
        Ok(())
    })?;
    Ok(())
}
