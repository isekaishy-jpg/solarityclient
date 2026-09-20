//! Shared archive ownership reaches the real decoder and memory-output voice engine.

use std::{error::Error, num::NonZeroUsize, sync::Arc};

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use solarity_media::{
    OwnedSoundEngine, SoundCategorySettings, SoundChannel, SoundEngineSettings, SoundGain,
    SoundLoadRequest, SoundLoopMode, SoundOutputTarget, SoundPlayback, SoundResidencyPolicy,
    SoundSoftwareChannelCount,
};

use super::{RuntimeSoundError, RuntimeSoundLoader, SoundLoadCompletion};
use crate::test_support::{ClientFixture, SDL_TEST_LOCK};

/// Direct-path tests still supply the engine's required three-column UI lookup table.
fn sound_fixture(files: &[(&str, &[u8])]) -> Result<ClientFixture, Box<dyn Error>> {
    let mut lookups = b"WDBC".to_vec();
    for word in [0_u32, 3, 12, 1] {
        lookups.extend_from_slice(&word.to_le_bytes());
    }
    lookups.push(0);
    let mut files = files.to_vec();
    files.push(("DBFilesClient/UISoundLookups.dbc", &lookups));
    ClientFixture::with_common_files(&files)
}

/// A deterministic mono PCM fixture needs no external audio files or device clock.
fn tone() -> Result<Vec<u8>, Box<dyn Error>> {
    let samples: Vec<i16> = [10000, -10000, 5000, -5000].repeat(2000);
    let size = u32::try_from(samples.len() * 2)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8000_u32.to_le_bytes());
    bytes.extend_from_slice(&16000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}

/// Memory output preserves the stock sample cache policy without audible playback.
fn engine(store: &mut AssetStore) -> Result<OwnedSoundEngine, Box<dyn Error>> {
    let full = SoundGain::new(1.)?;
    Ok(OwnedSoundEngine::load(
        store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(12),
        SoundEngineSettings::new(
            true,
            full,
            SoundCategorySettings::new(true, full),
            SoundCategorySettings::new(true, full),
            SoundCategorySettings::new(true, full),
            SoundResidencyPolicy::new(1 << 20, 16 << 20),
        ),
    )?)
}

/// One flexible lane provides deterministic FIFO completion markers.
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or("positive test capacity required")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// Reservation chooses its exact path before the loader receives ownership.
fn request(engine: &mut OwnedSoundEngine, path: &str) -> Result<SoundLoadRequest, Box<dyn Error>> {
    let path = AssetPath::new(path)?;
    Ok(engine
        .with_engine_mut(|engine| {
            engine.begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)
        })?
        .ok_or("eligible sound was suppressed")?)
}

/// Each marker completes earlier required reads/decodes without sleeps or timeouts.
fn completion(
    loader: &mut RuntimeSoundLoader,
    engine: &mut OwnedSoundEngine,
    cpu: &CpuExecutor,
) -> Result<SoundLoadCompletion, Box<dyn Error>> {
    for _ in 0..8 {
        if let Some(done) = engine.with_engine_mut(|engine| loader.poll(cpu, engine))? {
            return Ok(done);
        }
        cpu.try_submit(|| ())?.join()?;
    }
    Err("sound did not complete within its finite read/decode steps".into())
}

/// A/B/A preserves voice order while both A consumers pin the one archive result.
#[test]
fn shared_sound_reads_preserve_nonadjacent_voice_order_and_exact_payload_identity()
-> Result<(), Box<dyn Error>> {
    let _sdl = SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock is poisoned")?;
    let fixture = sound_fixture(&[("Sound/Test/Tone.wav", &tone()?)])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let mut engine = engine(&mut store)?;
    let mut cpu = pool()?;
    let mut loader = RuntimeSoundLoader::new(catalog.clone());
    let first = request(&mut engine, "Sound/Test/Tone.wav")?;
    let middle = request(&mut engine, "Sound/Test/Missing.wav")?;
    let last = request(&mut engine, "SOUND/TEST/TONE.WAV")?;
    let key = AssetResourceKey::new(catalog.namespace(), first.path().clone());
    for load in [&first, &middle, &last] {
        loader.queue(load.clone());
    }
    assert!(
        engine
            .with_engine_mut(|engine| loader.poll(&cpu, engine))?
            .is_none()
    );
    cpu.try_submit(|| ())?.join()?;
    assert!(loader.requests.poll().is_empty());
    let source = loader
        .requests
        .ready(&key, &first.handle())
        .ok_or("shared sound source is not ready")??;
    assert!(Arc::ptr_eq(
        &source,
        &loader
            .requests
            .ready(&key, &last.handle())
            .ok_or("shared sound source is not ready")??
    ));
    let done = completion(&mut loader, &mut engine, &cpu)?;
    assert_eq!(done.handle, first.handle());
    assert!(!first.is_pending());
    assert!(last.is_pending());
    assert!(matches!(done.result?, SoundPlayback::Started(_)));
    assert!(loader.requests.ready(&key, &first.handle()).is_none());
    let done = completion(&mut loader, &mut engine, &cpu)?;
    assert_eq!(done.handle, middle.handle());
    assert!(done.result.is_err());
    assert!(Arc::ptr_eq(
        &source,
        &loader
            .requests
            .ready(&key, &last.handle())
            .ok_or("shared sound source is not ready")??
    ));
    let done = completion(&mut loader, &mut engine, &cpu)?;
    assert_eq!(done.handle, last.handle());
    assert!(!last.is_pending());
    assert!(matches!(done.result?, SoundPlayback::Started(_)));
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 2);
    assert_eq!(engine.with_engine(|engine| engine.decoded_sound_count()), 1);
    engine.with_engine_mut(|engine| loader.shutdown(engine))?;
    cpu.shutdown()?;
    Ok(())
}

/// Cancelling the consumer that started a read cannot cancel its surviving peer.
#[test]
fn cancelling_the_first_sound_consumer_preserves_the_shared_read_for_the_second()
-> Result<(), Box<dyn Error>> {
    let _sdl = SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock is poisoned")?;
    let fixture = sound_fixture(&[("tone.wav", &tone()?)])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let mut engine = engine(&mut store)?;
    let mut cpu = pool()?;
    let mut loader = RuntimeSoundLoader::new(catalog);
    let first = request(&mut engine, "tone.wav")?;
    let second = request(&mut engine, "TONE.WAV")?;
    loader.queue(first.clone());
    loader.queue(second.clone());
    assert!(
        engine
            .with_engine_mut(|engine| loader.poll(&cpu, engine))?
            .is_none()
    );
    assert!(first.is_pending());
    engine.with_engine_mut(|engine| engine.cancel_load(first.handle()));
    assert!(!first.is_pending());
    assert!(second.is_pending());
    let done = completion(&mut loader, &mut engine, &cpu)?;
    assert_eq!(done.handle, second.handle());
    assert!(matches!(done.result?, SoundPlayback::Started(_)));
    assert!(!engine.with_engine(|engine| engine.is_load_pending(first.handle())));
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 1);
    engine.with_engine_mut(|engine| loader.shutdown(engine))?;
    cpu.shutdown()?;
    Ok(())
}

/// One archive failure belongs to both reservations without keeping a negative cache.
#[test]
fn shared_sound_failure_fans_out_in_voice_order() -> Result<(), Box<dyn Error>> {
    let _sdl = SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock is poisoned")?;
    let fixture = sound_fixture(&[])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let mut engine = engine(&mut store)?;
    let mut cpu = pool()?;
    let mut loader = RuntimeSoundLoader::new(catalog);
    let first = request(&mut engine, "missing.wav")?;
    let second = request(&mut engine, "MISSING.WAV")?;
    loader.queue(first.clone());
    loader.queue(second.clone());
    let a = completion(&mut loader, &mut engine, &cpu)?;
    let b = completion(&mut loader, &mut engine, &cpu)?;
    assert_eq!((a.handle, b.handle), (first.handle(), second.handle()));
    let (Err(RuntimeSoundError::SharedSource(a)), Err(RuntimeSoundError::SharedSource(b))) =
        (a.result, b.result)
    else {
        return Err("archive failure did not fan out through the shared source".into());
    };
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(engine.with_engine(|engine| engine.active_voice_count()), 0);
    assert!(loader.queued.is_empty());
    engine.with_engine_mut(|engine| loader.shutdown(engine))?;
    cpu.shutdown()?;
    Ok(())
}
