//! Nonblocking stock admission preserves selection, cancellation, and live gain.

#[path = "worker_loading.rs"]
mod worker_loading;

use std::cell::Cell;
use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    OwnedSoundEngine, SoundCache, SoundCategory, SoundChannel, SoundConcurrencyMode,
    SoundEngineError, SoundLoadRequest, SoundLoopMode, SoundOutputTarget, SoundPlayRequest,
    SoundPlayback, SoundSoftwareChannelCount, SoundVariationMode,
};

use crate::support::{
    Fixture, FixtureFile, empty_advanced_sound_entries_fixture, pcm_wav, sdl_test_lock,
    sound_entries_fixture_with_flags,
};

use super::settings;

/// The selected path may not exist yet: begin_load must perform no payload read.
#[test]
fn nonblocking_selection_precedes_reads_and_respects_pending_admission()
-> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let calls = Cell::new(0);
    let mut random = || {
        calls.set(calls.get() + 1);
        0
    };
    let capped = request(SoundChannel::new(7)?, SoundConcurrencyMode::Concurrent);
    let first = required(engine.with_engine_mut(|engine| engine.begin_load(capped, &mut random))?)?;
    let consumed = calls.get();
    assert!(consumed > 0);
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 0);
    assert_eq!(engine.with_engine(|engine| engine.cached_sound_count()), 0);
    assert_eq!(engine.with_engine(|engine| engine.decoded_sound_count()), 0);
    assert!(matches!(
        engine.with_engine_mut(|engine| engine.begin_load(capped, &mut random)),
        Err(SoundEngineError::ChannelCapacity {
            channel: 7,
            maximum: 1
        })
    ));
    assert!(matches!(
        engine.with_engine_mut(|engine| engine.play(&mut store, capped, &mut random)),
        Err(SoundEngineError::ChannelCapacity { .. })
    ));
    assert_eq!(calls.get(), consumed);
    assert!(engine.with_engine_mut(|engine| engine.cancel_load(first.handle())));
    assert!(!engine.with_engine_mut(|engine| engine.cancel_load(first.handle())));
    let exclusive = request(SoundChannel::SFX, SoundConcurrencyMode::Entry);
    let first =
        required(engine.with_engine_mut(|engine| engine.begin_load(exclusive, &mut random))?)?;
    let consumed = calls.get();
    assert!(matches!(
        engine.with_engine_mut(|engine| engine.begin_load(exclusive, &mut random)),
        Err(SoundEngineError::ExclusiveEntryActive { entry_id: 77 })
    ));
    assert_eq!(calls.get(), consumed);
    assert_eq!(
        engine.with_engine_mut(|engine| engine.stop_category(SoundCategory::Sfx))?,
        1
    );
    assert!(!engine.with_engine(|engine| engine.is_load_pending(first.handle())));
    engine.with_engine_mut(|engine| -> Result<_, Box<dyn Error>> {
        Ok(engine.set_settings(settings(false)?)?)
    })?;
    assert!(
        engine
            .with_engine_mut(|engine| engine.begin_load(exclusive, &mut random))?
            .is_none()
    );
    assert_eq!(calls.get(), consumed);
    Ok(())
}

/// Stop/category replacement consumes reservations before any late bytes reach SDL.
#[test]
fn cancelled_foreign_and_duplicate_completions_cannot_start_voices() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let invalid = AssetPath::new("Sound/Test/Invalid.wav")?;
    let load = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&invalid, SoundChannel::AMBIENCE, SoundLoopMode::Loop)
    })?)?;
    let mut cache = SoundCache::new();
    let corrupt = cache.load(&mut store, &invalid)?;
    engine.with_engine_mut(|engine| engine.stop_category(SoundCategory::Ambience))?;
    assert_eq!(
        engine.with_engine_mut(|engine| engine.complete_load(load.handle(), &corrupt))?,
        SoundPlayback::Suppressed
    );
    assert_eq!(engine.with_engine(|engine| engine.decoded_sound_count()), 0);

    let load = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&path, SoundChannel::AMBIENCE, SoundLoopMode::Loop)
    })?)?;
    let encoded = cache.load(&mut store, &path)?;
    let mut foreign = OwnedSoundEngine::load(
        &mut store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(12),
        settings(true)?,
    )?;
    assert_eq!(
        foreign.with_engine_mut(|engine| engine.complete_load(load.handle(), &encoded))?,
        SoundPlayback::Suppressed
    );
    assert!(engine.with_engine(|engine| engine.is_load_pending(load.handle())));
    let SoundPlayback::Started(voice) =
        engine.with_engine_mut(|engine| engine.complete_load(load.handle(), &encoded))?
    else {
        return Err("live request was suppressed".into());
    };
    assert_eq!(
        engine.with_engine_mut(|engine| engine.complete_load(load.handle(), &encoded))?,
        SoundPlayback::Suppressed
    );
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 1);
    engine.with_engine_mut(|engine| engine.stop(voice))?;
    Ok(())
}

/// A voice can finish on the output while its replacement is still decoding.
#[test]
fn completion_retires_a_voice_that_stopped_after_load_reservation() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let first = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Once)
    })?)?;
    let SoundPlayback::Started(first) =
        engine.with_engine_mut(|engine| engine.complete_load(first.handle(), &encoded))?
    else {
        return Err("first voice was suppressed".into());
    };
    let next = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Once)
    })?)?;
    // The fixture is one second long. Do not collect it between reservation
    // and completion: a device can finish the sample during any worker load.
    engine.with_engine(|engine| engine.generate(&mut vec![0; 44_100 * 4 * 2]))?;
    assert_eq!(
        engine.with_engine(|engine| engine.voice_state(first))?,
        solarity_media::SoundVoiceState::Stopped
    );
    let SoundPlayback::Started(next) =
        engine.with_engine_mut(|engine| engine.complete_load(next.handle(), &encoded))?
    else {
        return Err("replacement voice was suppressed".into());
    };
    // This previously reached the retired backend generation and terminated
    // the client's next world sound-settings update with UnknownVoice.
    let live_settings = settings(true)?;
    engine.with_engine_mut(|engine| engine.set_settings(live_settings))?;
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 1);
    assert!(matches!(
        engine.with_engine(|engine| engine.voice_state(first)),
        Err(SoundEngineError::UnknownVoice)
    ));
    assert_eq!(
        engine.with_engine(|engine| engine.voice_state(next))?,
        solarity_media::SoundVoiceState::Playing
    );
    engine.with_engine_mut(|engine| engine.stop(next))?;
    Ok(())
}

/// Completion consumes every failing reservation instead of keeping an exclusive ghost.
#[test]
fn failed_or_mismatched_loads_release_admission() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut cache = SoundCache::new();
    let invalid = AssetPath::new("Sound/Test/Invalid.wav")?;
    let corrupt = cache.load(&mut store, &invalid)?;
    let load = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&invalid, SoundChannel::SFX, SoundLoopMode::Once)
    })?)?;
    assert!(
        engine
            .with_engine_mut(|engine| engine.complete_load(load.handle(), &corrupt))
            .is_err()
    );
    assert!(!engine.with_engine(|engine| engine.is_load_pending(load.handle())));
    let load = required(engine.with_engine_mut(|engine| {
        engine.begin_load(
            request(SoundChannel::SFX, SoundConcurrencyMode::Entry),
            &mut || 0,
        )
    })?)?;
    assert!(matches!(
        engine.with_engine_mut(|engine| engine.complete_load(load.handle(), &corrupt)),
        Err(SoundEngineError::LoadPathMismatch { .. })
    ));
    assert!(!engine.with_engine(|engine| engine.is_load_pending(load.handle())));
    // The DBC's Missing.wav is intentionally absent; the immediate API must also
    // cancel its reservation after an exact archive failure.
    assert!(matches!(
        engine.with_engine_mut(|engine| engine.play(
            &mut store,
            request(SoundChannel::SFX, SoundConcurrencyMode::Entry),
            &mut || 0
        )),
        Err(SoundEngineError::Asset(_))
    ));
    let retry = required(engine.with_engine_mut(|engine| {
        engine.begin_load(
            request(SoundChannel::SFX, SoundConcurrencyMode::Entry),
            &mut || 0,
        )
    })?)?;
    engine.with_engine_mut(|engine| engine.cancel_load(retry.handle()));
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 0);
    assert_eq!(engine.with_engine(|engine| engine.decoded_sound_count()), 0);
    Ok(())
}

/// Disabling after selection mutes accepted playback; re-enabling does not reselect it.
#[test]
fn accepted_load_uses_live_gain_at_completion() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let load = required(engine.with_engine_mut(|engine| {
        engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)
    })?)?;
    engine.with_engine_mut(|engine| -> Result<_, Box<dyn Error>> {
        Ok(engine.set_settings(settings(false)?)?)
    })?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let SoundPlayback::Started(voice) =
        engine.with_engine_mut(|engine| engine.complete_load(load.handle(), &encoded))?
    else {
        return Err("accepted request was discarded on mute".into());
    };
    let mut pcm = [0xff; 4096];
    engine.with_engine(|engine| engine.generate(&mut pcm))?;
    assert!(pcm.iter().all(|byte| *byte == 0));
    engine.with_engine_mut(|engine| -> Result<_, Box<dyn Error>> {
        Ok(engine.set_settings(settings(true)?)?)
    })?;
    engine.with_engine(|engine| engine.generate(&mut pcm))?;
    assert!(pcm.iter().any(|byte| *byte != 0));
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 1);
    engine.with_engine_mut(|engine| engine.stop(voice))?;
    Ok(())
}

/// Contains a missing DBC variation and exact valid/corrupt direct-path payloads.
fn assets() -> Result<(Fixture, AssetStore), Box<dyn Error>> {
    let entries = sound_entries_fixture_with_flags(
        77,
        [("Missing.wav", 1), ("OtherMissing.wav", 1), ("", 0)],
        "Sound/Test",
        0x20,
    );
    let wav = pcm_wav(8000, &[0, 12000, 0, -12000].repeat(2000))?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &entries,
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
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound\\Test\\Invalid.wav",
            bytes: b"invalid audio",
        },
    ])?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((fixture, store))
}

/// Owns a memory output so tests verify actual mixed PCM without a device or clock.
fn engine(store: &mut AssetStore) -> Result<OwnedSoundEngine, Box<dyn Error>> {
    Ok(OwnedSoundEngine::load(
        store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(12),
        settings(true)?,
    )?)
}

/// All requests here are eligible; suppression is a test failure.
fn required(load: Option<SoundLoadRequest>) -> Result<SoundLoadRequest, Box<dyn Error>> {
    load.ok_or_else(|| "eligible sound load was suppressed".into())
}

/// Random variation exercises the caller-owned RNG before asynchronous extraction.
fn request(channel: SoundChannel, mode: SoundConcurrencyMode) -> SoundPlayRequest {
    SoundPlayRequest::new(
        77,
        channel,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        mode,
    )
}
