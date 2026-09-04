//! Worker decoder ownership, admission pressure, cancellation, and actual playback.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{Arc, mpsc};

use solarity_asset::AssetPath;
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_media::{
    EncodedSound, OwnedSoundEngine, SoundCache, SoundCategory, SoundChannel, SoundEngineError,
    SoundLoadRequest, SoundLoopMode, SoundPlayback,
};

use crate::support::sdl_test_lock;

use super::super::{settings, settings_with_residency};
use super::{assets, engine, required};

/// Worker completion obeys live gain; a retained sample needs no second worker job.
#[test]
fn worker_sample_completion_uses_live_gain_and_reuses_retained_audio() -> Result<(), Box<dyn Error>>
{
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut cpu = pool(1)?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let load = required(engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
    assert_eq!(engine.poll_load(&cpu, load.handle(), &encoded)?, None);
    assert_eq!(engine.active_voice_count(), 0);
    engine.set_settings(settings(false)?)?;
    cpu.shutdown()?;
    let SoundPlayback::Started(first) = finish_prepared(&mut engine, &cpu, &load, &encoded)? else {
        return Err("finished worker sample was not admitted".into());
    };
    let mut pcm = [0xff; 4096];
    engine.generate(&mut pcm)?;
    assert!(pcm.iter().all(|&byte| byte == 0));
    engine.set_settings(settings(true)?)?;
    engine.generate(&mut pcm)?;
    assert!(pcm.iter().any(|&byte| byte != 0));
    assert_eq!(
        engine.poll_load(&cpu, load.handle(), &encoded)?,
        Some(SoundPlayback::Suppressed)
    );

    let load = required(engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
    // Admission must work even though this pool no longer accepts work.
    let Some(SoundPlayback::Started(second)) = engine.poll_load(&cpu, load.handle(), &encoded)?
    else {
        return Err("retained sample required another decode".into());
    };
    assert_eq!(engine.decoded_sound_count(), 1);
    engine.stop(first)?;
    engine.stop(second)?;
    Ok(())
}

/// Sample completions deduplicate even when both jobs precede registry admission.
/// Streams retain separate resources and release each one with its own voice.
#[test]
fn concurrent_worker_completions_preserve_sample_and_stream_residency() -> Result<(), Box<dyn Error>>
{
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    for cacheable_bytes in [0, 1_048_576] {
        let mut engine = engine(&mut store)?;
        engine.set_settings(settings_with_residency(true, cacheable_bytes)?)?;
        let mut cpu = pool(2)?;
        let first =
            required(engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
        let second =
            required(engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
        assert_eq!(engine.poll_load(&cpu, first.handle(), &encoded)?, None);
        assert_eq!(engine.poll_load(&cpu, second.handle(), &encoded)?, None);
        cpu.shutdown()?;
        let SoundPlayback::Started(first) = finish_prepared(&mut engine, &cpu, &first, &encoded)?
        else {
            return Err("first prepared voice was suppressed".into());
        };
        let SoundPlayback::Started(second) = finish_prepared(&mut engine, &cpu, &second, &encoded)?
        else {
            return Err("second prepared voice was suppressed".into());
        };
        assert_eq!(
            engine.decoded_sound_count(),
            if cacheable_bytes == 0 { 2 } else { 1 }
        );
        engine.stop(first)?;
        assert_eq!(engine.decoded_sound_count(), 1);
        let mut pcm = [0; 4096];
        engine.generate(&mut pcm)?;
        assert!(pcm.iter().any(|&byte| byte != 0));
        engine.stop(second)?;
        assert_eq!(
            engine.decoded_sound_count(),
            usize::from(cacheable_bytes != 0)
        );
    }
    Ok(())
}

/// Full CPU admission keeps the exact request pending and performs no frame-thread decode.
#[test]
fn worker_backpressure_preserves_reservation_until_cancelled() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let cpu = pool(1)?;
    // The sender drops before the pool on an assertion failure, unblocking shutdown.
    let (release, wait) = mpsc::channel::<()>();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let channel = SoundChannel::new(7)?;
    let load = required(engine.begin_file_load(&path, channel, SoundLoopMode::Loop)?)?;
    assert_eq!(engine.poll_load(&cpu, load.handle(), &encoded)?, None);
    assert!(engine.is_load_pending(load.handle()));
    assert_eq!(engine.decoded_sound_count(), 0);
    assert!(matches!(
        engine.begin_file_load(&path, channel, SoundLoopMode::Loop),
        Err(SoundEngineError::ChannelCapacity { .. })
    ));
    assert!(engine.cancel_load(load.handle()));
    release.send(())?;
    blocker.join()??;
    assert_eq!(
        engine.poll_load(&cpu, load.handle(), &encoded)?,
        Some(SoundPlayback::Suppressed)
    );
    assert_eq!(engine.decoded_sound_count(), 0);
    Ok(())
}

/// Cancellation while a decode is queued keeps task ownership but never starts a track.
#[test]
fn cancelled_queued_decode_is_observed_and_discarded() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let cpu = pool(2)?;
    let (release, wait) = mpsc::channel::<()>();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let load =
        required(engine.begin_file_load(&path, SoundChannel::AMBIENCE, SoundLoopMode::Loop)?)?;
    assert_eq!(engine.poll_load(&cpu, load.handle(), &encoded)?, None);
    assert_eq!(cpu.snapshot()?.in_flight(), 2);
    assert_eq!(engine.stop_category(SoundCategory::Ambience)?, 1);
    release.send(())?;
    blocker.join()??;
    engine.finish_cancelled_loads();
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    assert_eq!(engine.decoded_sound_count(), 0);
    assert_eq!(engine.active_voice_count(), 0);
    assert_eq!(
        engine.poll_load(&cpu, load.handle(), &encoded)?,
        Some(SoundPlayback::Suppressed)
    );
    Ok(())
}

/// Corrupt worker input consumes the reservation without retaining a resource or retrying.
#[test]
fn failed_worker_decode_releases_admission() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut cpu = pool(1)?;
    let path = AssetPath::new("Sound/Test/Invalid.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let load = required(engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Once)?)?;
    assert_eq!(engine.poll_load(&cpu, load.handle(), &encoded)?, None);
    cpu.shutdown()?;
    assert!(matches!(
        finish_prepared(&mut engine, &cpu, &load, &encoded),
        Err(SoundEngineError::Decode(_))
    ));
    assert!(!engine.is_load_pending(load.handle()));
    assert_eq!(engine.decoded_sound_count(), 0);
    assert_eq!(engine.active_voice_count(), 0);
    Ok(())
}

/// A caller may drop the engine before polling completion; its worker must still retire.
#[test]
fn dropping_engine_joins_decode_before_releasing_sdl_initialization() -> Result<(), Box<dyn Error>>
{
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut first = engine(&mut store)?;
    let cpu = pool(2)?;
    let (release, wait) = mpsc::channel::<()>();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;
    let load = required(first.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
    assert_eq!(first.poll_load(&cpu, load.handle(), &encoded)?, None);
    release.send(())?;
    drop(first);
    blocker.join()??;
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    // Full SDL_mixer teardown/reinitialization must leave no late Audio destructor.
    let mut second = engine(&mut store)?;
    let load = required(second.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?)?;
    let SoundPlayback::Started(voice) = second.complete_load(load.handle(), &encoded)? else {
        return Err("reinitialized engine could not play".into());
    };
    second.stop(voice)?;
    Ok(())
}

/// One worker makes queueing deterministic; capacity includes queued and running work.
fn pool(capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::new(1).ok_or("invalid worker count")?,
        NonZeroUsize::new(capacity).ok_or("invalid admission capacity")?,
    ))?)
}

/// CPU shutdown observes completed work before its completion channel publishes.
/// Poll that final publication without assuming an elapsed-time scheduling bound.
fn finish_prepared(
    engine: &mut OwnedSoundEngine,
    cpu: &CpuExecutor,
    load: &SoundLoadRequest,
    encoded: &Arc<EncodedSound>,
) -> Result<SoundPlayback, SoundEngineError> {
    loop {
        if let Some(playback) = engine.poll_load(cpu, load.handle(), encoded)? {
            return Ok(playback);
        }
        std::thread::yield_now();
    }
}
