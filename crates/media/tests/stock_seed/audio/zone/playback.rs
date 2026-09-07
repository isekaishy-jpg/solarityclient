//! Playback regressions through real MPQ payloads and SDL's memory mixer.

use super::{frame, table};
use crate::support::{
    Fixture, FixtureFile, empty_advanced_sound_entries_fixture, pcm_wav, sdl_test_lock,
    sound_entries_fixture,
};
use solarity_asset::{
    ArchiveCatalog, AreaSoundReferences, AssetStore, ClientDataRoot, Locale, ZoneSoundCatalog,
};
use solarity_media::{
    OwnedSoundEngine, SoundCache, SoundCategorySettings, SoundChannel, SoundEngine,
    SoundEngineSettings, SoundFade, SoundFadeDirection, SoundGain, SoundLoadRequest, SoundLoopMode,
    SoundOutputTarget, SoundPlayback, SoundResidencyPolicy, SoundSoftwareChannelCount,
    ZoneSoundLayer, ZoneSoundService,
};
use std::error::Error;
use std::time::Duration;

/// The first SDL buffer of a queued fade-in is silent, including the interval
/// before the service receives its completion callback.
#[test]
fn pending_fade_is_applied_before_backend_admission() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    engine.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        let path = solarity_asset::AssetPath::new("Sound/Test/Tone.wav")?;
        let load = engine
            .begin_file_load(&path, SoundChannel::AMBIENCE, SoundLoopMode::Loop)?
            .ok_or("load")?;
        let mut fade = SoundFade::new(SoundGain::MUTED);
        fade.retarget_seconds(SoundFadeDirection::In, 2.0);
        assert!(engine.set_load_fade(load.handle(), fade));
        let encoded = SoundCache::new().load(&mut store, &path)?;
        let SoundPlayback::Started(voice) = engine.complete_load(load.handle(), &encoded)? else {
            return Err("voice".into());
        };
        assert!(!engine.set_load_fade(load.handle(), SoundFade::default()));
        let mut pcm = [0u8; 4096];
        engine.generate(&mut pcm)?;
        assert!(pcm.iter().all(|byte| *byte == 0));
        engine.advance_fades(Duration::from_secs(1))?;
        assert_eq!(engine.voice_fade(voice)?.gain(), 0.5);
        engine.generate(&mut pcm)?;
        assert!(pcm.iter().any(|byte| *byte != 0));
        Ok(())
    })
}

/// 4C5CC0/87A920 stops the SFX channel without stopping other channels that
/// happen to share the SFX volume slider, and late loads cannot resurrect it.
#[test]
fn stop_all_sfx_fades_only_channel_zero_and_cancels_its_loads() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    engine.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        let path = solarity_asset::AssetPath::new("Sound/Test/Tone.wav")?;
        let encoded = SoundCache::new().load(&mut store, &path)?;
        let mut voices = Vec::new();
        for channel in [
            SoundChannel::SFX,
            SoundChannel::OTHER_FOOTSTEP,
            SoundChannel::MUSIC,
        ] {
            let load = engine
                .begin_file_load(&path, channel, SoundLoopMode::Loop)?
                .ok_or("load")?;
            let SoundPlayback::Started(voice) = engine.complete_load(load.handle(), &encoded)?
            else {
                return Err("voice".into());
            };
            voices.push(voice);
        }
        let pending = engine
            .begin_file_load(&path, SoundChannel::SFX, SoundLoopMode::Loop)?
            .ok_or("pending")?;
        engine.fade_channel_out(SoundChannel::SFX, 2.0)?;
        assert!(!engine.is_load_pending(pending.handle()));
        assert!(matches!(
            engine.complete_load(pending.handle(), &encoded)?,
            SoundPlayback::Suppressed
        ));
        engine.advance_fades(Duration::from_secs(1))?;
        assert_eq!(engine.voice_fade(voices[0])?.gain(), 0.5);
        assert_eq!(engine.voice_fade(voices[1])?.gain(), 1.0);
        assert_eq!(engine.voice_fade(voices[2])?.gain(), 1.0);
        engine.advance_fades(Duration::from_secs(1))?;
        assert_eq!(engine.active_voice_count(), 2);
        assert!(matches!(
            engine.voice_fade(voices[0]),
            Err(solarity_media::SoundEngineError::UnknownVoice)
        ));
        Ok(())
    })
}

/// Reentering A during A->B revives A's exact voice and its current envelope.
#[test]
fn zone_crossfade_revives_previous_generation_without_restarting() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut zones = ZoneSoundService::new(ZoneSoundCatalog::load(&mut store)?);
    engine.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        set_zone(&mut zones, 11, 0);
        let loads = zones.update(engine, frame(0), false, &mut || 0)?;
        complete(engine, &mut zones, &mut store, &loads)?;
        let a = zones.music_voice().ok_or("zone A voice")?;
        assert_eq!(engine.voice_fade(a)?.gain(), 0.0);
        engine.advance_fades(Duration::from_millis(10))?;
        assert_eq!(engine.voice_fade(a)?.gain(), 1.0);
        set_zone(&mut zones, 12, 10);
        let loads = zones.update(engine, frame(10), false, &mut || 0)?;
        complete(engine, &mut zones, &mut store, &loads)?;
        let b = zones.music_voice().ok_or("zone B voice")?;
        assert_ne!(a, b);
        engine.advance_fades(Duration::from_secs(1))?;
        assert_eq!(engine.voice_fade(a)?.gain(), 0.75);
        assert_eq!(engine.voice_fade(b)?.gain(), 0.25);
        set_zone(&mut zones, 11, 1010);
        assert!(
            zones
                .update(engine, frame(1010), false, &mut || 0)?
                .is_empty()
        );
        assert_eq!(zones.music_voice(), Some(a));
        assert_eq!(engine.voice_fade(a)?.gain(), 0.75);
        engine.advance_fades(Duration::from_secs(1))?;
        assert_eq!(engine.voice_fade(a)?.gain(), 1.0);
        assert!(
            zones
                .update(engine, frame(2010), false, &mut || 0)?
                .is_empty()
        );
        assert_eq!(engine.active_voice_count(), 1);
        let mut pcm = [0u8; 4096];
        engine.generate(&mut pcm)?;
        assert!(pcm.iter().any(|byte| *byte != 0));
        zones.clear(engine)?;
        assert_eq!(engine.active_voice_count(), 0);
        Ok(())
    })
}

/// Load completion after a border crossing or disconnect cannot resurrect a cue.
#[test]
fn zone_replacement_and_teardown_cancel_pending_payloads() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut zones = ZoneSoundService::new(ZoneSoundCatalog::load(&mut store)?);
    engine.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        set_zone(&mut zones, 11, 0);
        let first = zones.update(engine, frame(0), false, &mut || 0)?;
        set_zone(&mut zones, 12, 1);
        let second = zones.update(engine, frame(1), false, &mut || 0)?;
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        let encoded = SoundCache::new().load(&mut store, first[0].path())?;
        assert_eq!(
            engine.complete_load(first[0].handle(), &encoded)?,
            SoundPlayback::Suppressed
        );
        zones.clear(engine)?;
        assert_eq!(
            engine.complete_load(second[0].handle(), &encoded)?,
            SoundPlayback::Suppressed
        );
        assert_eq!(engine.active_voice_count(), 0);
        Ok(())
    })
}

/// Actual output completion starts the authored delay, then permits a new track.
#[test]
fn zone_natural_completion_obeys_authored_silence() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut store) = assets()?;
    let _sdl = sdl_test_lock();
    let mut engine = engine(&mut store)?;
    let mut zones = ZoneSoundService::new(ZoneSoundCatalog::load(&mut store)?);
    engine.with_engine_mut(|engine| -> Result<(), Box<dyn Error>> {
        set_zone(&mut zones, 11, 0);
        let loads = zones.update(engine, frame(0), false, &mut || 0)?;
        complete(engine, &mut zones, &mut store, &loads)?;
        engine.advance_fades(Duration::from_millis(10))?;
        let mut pcm = vec![0u8; 44100 * 8 * 2];
        engine.generate(&mut pcm)?;
        assert!(
            zones
                .update(engine, frame(100), false, &mut || 0)?
                .is_empty()
        );
        assert!(zones.music_voice().is_none());
        assert!(
            zones
                .update(engine, frame(1099), false, &mut || 0)?
                .is_empty()
        );
        assert_eq!(
            zones.update(engine, frame(1100), false, &mut || 0)?.len(),
            1
        );
        zones.clear(engine)?;
        Ok(())
    })
}

/// Completes each selected fixture payload through the public engine boundary.
fn complete(
    engine: &mut SoundEngine<'_>,
    zones: &mut ZoneSoundService,
    store: &mut AssetStore,
    loads: &[SoundLoadRequest],
) -> Result<(), Box<dyn Error>> {
    let mut cache = SoundCache::new();
    for load in loads {
        let encoded = cache.load(store, load.path())?;
        let playback = engine.complete_load(load.handle(), &encoded)?;
        zones.complete_load(engine, load.handle(), playback)?;
    }
    Ok(())
}

/// Changes only the area music relation, preserving selection clock semantics.
fn set_zone(zones: &mut ZoneSoundService, id: u32, now: u32) {
    zones.set_location(
        ZoneSoundLayer::Area,
        Some(AreaSoundReferences {
            zone_music_id: id,
            ..Default::default()
        }),
        frame(now),
    );
}

/// Uses explicit policy and memory output; no machine sound device is required.
fn engine(store: &mut AssetStore) -> Result<OwnedSoundEngine, Box<dyn Error>> {
    let full = SoundGain::new(1.0)?;
    Ok(OwnedSoundEngine::load(
        store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(64),
        SoundEngineSettings::new(
            true,
            full,
            SoundCategorySettings::new(true, full),
            SoundCategorySettings::new(true, full),
            SoundCategorySettings::new(true, full),
            SoundResidencyPolicy::new(2097152, 16777216),
        ),
    )?)
}

/// Two sound identities share one PCM payload; the DBC namespace stays distinct.
fn assets() -> Result<(Fixture, AssetStore), Box<dyn Error>> {
    let single = sound_entries_fixture(101, [("Tone.wav", 1), ("", 0), ("", 0)], "Sound\\Test");
    let mut sounds = single[..20].to_vec();
    sounds[4..8].copy_from_slice(&2u32.to_le_bytes());
    sounds.extend_from_slice(&single[20..140]);
    let mut second = single[20..140].to_vec();
    second[..4].copy_from_slice(&201u32.to_le_bytes());
    sounds.extend_from_slice(&second);
    sounds.extend_from_slice(&single[140..]);
    let music = table(
        8,
        &[
            11, 0, 1000, 1000, 1000, 1000, 101, 101, 12, 0, 0, 0, 0, 0, 201, 201,
        ],
    );
    let intro = table(5, &[]);
    let ambience = table(3, &[]);
    let advanced = empty_advanced_sound_entries_fixture();
    let pcm = pcm_wav(44100, &vec![12000; 44100])?;
    let files = [
        ("DBFilesClient\\SoundEntries.dbc", sounds.as_slice()),
        (
            "DBFilesClient\\SoundEntriesAdvanced.dbc",
            advanced.as_slice(),
        ),
        ("DBFilesClient\\ZoneMusic.dbc", music.as_slice()),
        ("DBFilesClient\\ZoneIntroMusicTable.dbc", intro.as_slice()),
        ("DBFilesClient\\SoundAmbience.dbc", ambience.as_slice()),
        ("Sound\\Test\\Tone.wav", pcm.as_slice()),
    ];
    let files: Vec<_> = files
        .iter()
        .map(|(path, bytes)| FixtureFile {
            archive: "common.MPQ",
            path,
            bytes,
        })
        .collect();
    let fixture = Fixture::new(&files)?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok((fixture, store))
}
