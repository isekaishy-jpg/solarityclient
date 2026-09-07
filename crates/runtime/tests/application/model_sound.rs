//! Native model callback ownership reaches the actual archive worker and mixer.

use super::super::{
    RuntimeGlueVoice, RuntimeGlueVoiceIdentity, RuntimeSoundLoader, SoundCvarSource, SoundPolicy,
    output::RuntimeSoundOutput,
};
use super::{M2SoundKind, M2SoundOwner, ModelPlayback, RuntimeM2Event, RuntimeSoundCoordinator};
use crate::random::BlizzardRand;
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_media::{
    AdvancedSoundService, OwnedSoundEngine, SoundChannel, SoundConcurrencyMode, SoundEngineError,
    SoundLoopMode, SoundOutputTarget, SoundPlayRequest, SoundPlayback, SoundSoftwareChannelCount,
    SoundVariationMode, SoundVoiceState,
};
use solarity_rendering::{WorldCamera, WorldCameraFrame};
use std::error::Error;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Compare the request and fades to original callback output, including the
/// exact +0x18/+0x1c distinction that decompiler local names can obscure.
#[test]
fn model_loop_options_match_native_callbacks() -> Result<(), Box<dyn Error>> {
    let mut checked = 0;
    for line in include_str!("../fixtures/model-sound-native.txt").lines() {
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.get(1..5) != Some(&["$DSL", "0", "0", "play"][..]) {
            continue;
        }
        let kind = match fields[0] {
            "doodad" => M2SoundKind::Doodad,
            "gameobject" => M2SoundKind::GameObject,
            _ => return Err("unexpected callback owner".into()),
        };
        let options = fields[7..]
            .iter()
            .map(|value| u32::from_str_radix(value, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let request = super::callback_request(77, Some(kind));
        assert_eq!(u32::from(request.channel().value()), options[0]);
        assert_eq!(kind.fade_in_seconds().to_bits(), options[4]);
        assert_eq!(kind.fade_out_seconds().to_bits(), options[5]);
        assert_eq!(request.variation_mode() as u32, options[6]);
        assert_eq!(request.loop_mode().is_looping(0), options[7] == 1);
        checked += 1;
    }
    assert_eq!(checked, 2);
    assert!(
        !super::callback_request(77, None)
            .loop_mode()
            .is_looping(0x200)
    );
    Ok(())
}

struct Cvars;

impl SoundCvarSource for Cvars {
    fn sound_cvar(&self, _name: &str) -> Option<String> {
        Some("1".into())
    }
    fn publish_sound_output(
        &self,
        _names: Vec<String>,
        _index: usize,
        _name: &str,
    ) -> Result<(), solarity_ui::UiScriptError> {
        Err(solarity_ui::UiScriptError::Execution {
            label: "model sound fixture".into(),
            message: "memory output cannot publish physical devices".into(),
        })
    }
}

/// Both native callbacks force a nonlooping footstep entry to loop, suppress
/// repeated handles/nearby channels, and cancel or fade on exact model removal.
#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_model_loops_obey_callback_and_model_lifetimes() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let mut sound = RuntimeSoundCoordinator {
        output: RuntimeSoundOutput::resolve(
            &Cvars,
            SoundOutputTarget::Memory,
            "System Default".into(),
        )?,
        engine: OwnedSoundEngine::load(
            &mut store,
            SoundOutputTarget::Memory,
            SoundSoftwareChannelCount::new(64),
            SoundPolicy::read(&Cvars)?.settings,
        )?,
        movement_sounds: solarity_asset::MovementSoundCatalog::load(&mut store)?,
        zone: solarity_media::ZoneSoundService::new(solarity_asset::ZoneSoundCatalog::load(
            &mut store,
        )?),
        zone_overrides: solarity_asset::ZoneSoundOverrideCatalog::load(&mut store)?,
        assets: AssetStoreHandle::new(store),
        loader: RuntimeSoundLoader::new(catalog),
        model_sounds: Vec::new(),
        unit_vocals: Vec::new(),
        advanced: AdvancedSoundService::new(),
        glue_music: None,
        glue_music_repeat: None,
        glue_ambience: None,
        resident_tile: None,
        staged_emitters: None,
        last_update: Instant::now(),
        movement_events: Default::default(),
        water_splashes: Default::default(),
        movement_loads: Vec::new(),
        movement_voices: Vec::new(),
        world_listener: None,
        zone_references: None,
        next_zone_references: None,
        chunk_references: None,
        next_chunk_references: None,
        state_references: None,
        next_state_references: None,
        started: Instant::now(),
        last_fade_update: Instant::now(),
    };
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    let camera = WorldCamera::stock(Vec3::new(-5., 0., 2.), Vec3::Z, Vec3::Z, 100.).frame(1.)?;
    let mut random = BlizzardRand::new(1);
    for kind in [M2SoundKind::Doodad, M2SoundKind::GameObject] {
        exercise_loop(&mut sound, &cpu, camera, &mut random, kind)?;
    }
    exercise_glue_world_handoff(&mut sound, &mut random)?;
    sound.shutdown()?;
    cpu.shutdown()?;
    Ok(())
}

/// 4DAB40 releases the repeating Glue owner and leaves only its three-second tail.
fn exercise_glue_world_handoff(
    sound: &mut RuntimeSoundCoordinator,
    random: &mut BlizzardRand,
) -> Result<(), Box<dyn Error>> {
    let request = SoundPlayRequest::new(
        15168,
        SoundChannel::MUSIC,
        SoundVariationMode::Random,
        SoundLoopMode::Loop,
        SoundConcurrencyMode::Concurrent,
    );
    let pending = sound
        .engine
        .with_engine_mut(|engine| engine.begin_load(request, &mut || 0))?
        .ok_or("music load was suppressed")?;
    let pending_handle = pending.handle();
    sound.glue_music =
        sound.queue_glue_voice(RuntimeGlueVoiceIdentity::SoundEntry(15168), Some(pending));
    sound.glue_music_repeat = Some(15168);
    sound.enter_world()?;
    assert!(
        !sound
            .engine
            .with_engine(|engine| engine.is_load_pending(pending_handle))
    );
    sound.repeat_glue_music(random)?;
    assert!(sound.glue_music.is_none());
    assert!(sound.glue_music_repeat.is_none());

    let playback = sound.engine.with_engine_mut(|engine| {
        engine.play(&mut sound.assets.borrow_mut(), request, &mut || 0)
    })?;
    let SoundPlayback::Started(voice) = playback else {
        return Err("music was suppressed".into());
    };
    sound.glue_music =
        RuntimeGlueVoice::started(RuntimeGlueVoiceIdentity::SoundEntry(15168), playback);
    sound.glue_music_repeat = Some(15168);
    sound.enter_world()?;
    sound.repeat_glue_music(random)?;
    assert!(sound.glue_music.is_none());
    assert!(sound.glue_music_repeat.is_none());
    sound
        .engine
        .with_engine_mut(|engine| engine.advance_fades(Duration::from_secs(1)))?;
    let gain = sound
        .engine
        .with_engine(|engine| engine.voice_fade(voice))?
        .gain();
    assert!((gain - 2.0 / 3.0).abs() < 1.0e-6);
    sound
        .engine
        .with_engine_mut(|engine| engine.advance_fades(Duration::from_secs(2)))?;
    // Native envelopes store their gain to f32 after each tick. The rounded
    // one-third decrement can leave a tiny positive remainder at three seconds;
    // the following tick must retire it instead of restarting the Glue kit.
    assert!(
        sound
            .engine
            .with_engine(|engine| engine.voice_fade(voice))?
            .gain()
            < 1.0e-6
    );
    sound
        .engine
        .with_engine_mut(|engine| engine.advance_fades(Duration::from_millis(1)))?;
    assert!(matches!(
        sound.engine.with_engine(|engine| engine.voice_state(voice)),
        Err(SoundEngineError::UnknownVoice)
    ));
    Ok(())
}

fn event(identifier: [u8; 4], owner: &Rc<M2SoundKind>, position: Vec3) -> RuntimeM2Event {
    RuntimeM2Event::new(identifier, 15168, position, None)
        .with_sound_owner(M2SoundOwner::new(owner))
}

fn finish_loads(
    sound: &mut RuntimeSoundCoordinator,
    cpu: &CpuExecutor,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while sound
        .model_sounds
        .iter()
        .any(|sound| matches!(sound.playback, ModelPlayback::Loading(_)))
    {
        sound.poll_loads(cpu)?;
        if Instant::now() > deadline {
            return Err("model sound worker timeout".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn exercise_loop(
    sound: &mut RuntimeSoundCoordinator,
    cpu: &CpuExecutor,
    camera: WorldCameraFrame,
    random: &mut BlizzardRand,
    kind: M2SoundKind,
) -> Result<(), Box<dyn Error>> {
    let removed = Rc::new(kind);
    let old_event = event(*b"$DSL", &removed, Vec3::ZERO);
    assert_eq!(
        sound.play_m2_events(&[old_event.clone(), old_event.clone()], camera, random)?,
        1
    );
    assert_eq!(sound.model_sounds.len(), 1);
    drop(removed);
    sound.poll_loads(cpu)?;
    assert!(sound.model_sounds.is_empty());
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count()),
        0
    );

    let owner = Rc::new(kind);
    let start = event(*b"$DSL", &owner, Vec3::ZERO);
    assert!(
        !old_event
            .sound_owner()
            .ok_or("old owner")?
            .same_model(start.sound_owner().ok_or("new owner")?)
    );
    sound.play_m2_events(std::slice::from_ref(&start), camera, random)?;
    finish_loads(sound, cpu)?;
    assert_eq!(sound.model_sounds.len(), 1);
    let ModelPlayback::Playing(voice) = sound.model_sounds[0].playback else {
        return Err("loop did not load".into());
    };
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.voice_fade(voice))?
            .gain(),
        0.0
    );
    sound.engine.with_engine_mut(|engine| {
        engine.advance_fades(Duration::from_secs_f32(kind.fade_in_seconds()))
    })?;
    let mut mixed = vec![0; 44_100 * 4 * 10];
    sound
        .engine
        .with_engine_mut(|engine| engine.generate(&mut mixed))?;
    assert!(mixed.iter().any(|sample| *sample != 0));
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.voice_state(voice))?,
        SoundVoiceState::Playing
    );
    assert_eq!(
        sound.play_m2_events(std::slice::from_ref(&start), camera, random)?,
        0
    );

    let neighbor = Rc::new(kind);
    assert_eq!(
        sound.play_m2_events(
            &[event(*b"$DSL", &neighbor, Vec3::new(2., 1., 0.))],
            camera,
            random
        )?,
        0
    );
    // Exact integer coordinates avoid rounding sqrt(6) at the strict boundary.
    assert_eq!(
        sound.play_m2_events(
            &[event(*b"$DSL", &neighbor, Vec3::new(1., 1., 2.))],
            camera,
            random
        )?,
        1
    );
    finish_loads(sound, cpu)?;
    assert_eq!(sound.model_sounds.len(), 2);
    drop(owner);
    sound.collect_model_sounds()?;
    assert_eq!(sound.model_sounds.len(), 1);
    if kind == M2SoundKind::GameObject {
        assert_eq!(
            sound
                .engine
                .with_engine(|engine| engine.voice_state(voice))?,
            SoundVoiceState::Playing
        );
    }
    sound
        .engine
        .with_engine_mut(|engine| engine.advance_fades(Duration::from_secs(1)))?;
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count()),
        1
    );

    let stopped =
        sound.play_m2_events(&[event(*b"$DSE", &neighbor, Vec3::ZERO)], camera, random)?;
    assert_eq!(stopped, usize::from(kind == M2SoundKind::Doodad));
    drop(neighbor);
    sound.collect_model_sounds()?;
    sound
        .engine
        .with_engine_mut(|engine| engine.advance_fades(Duration::from_secs(1)))?;
    assert!(sound.model_sounds.is_empty());
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count()),
        0
    );
    Ok(())
}
