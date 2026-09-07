//! Main-thread sound output, live CVar policy, and terrain-emitter ownership.

use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStoreHandle, DecodedTerrainTile, TerrainTileIndex,
};
use solarity_cpu::{CpuError, CpuExecutor};
use solarity_media::{
    AdvancedSoundCreateRequest, AdvancedSoundListener, AdvancedSoundService,
    AdvancedSoundServiceError, OwnedSoundEngine, SoundCategorySettings, SoundChannel,
    SoundConcurrencyMode, SoundEngineError, SoundEngineSettings, SoundFade, SoundFadeDirection,
    SoundGain, SoundLoadHandle, SoundLoadRequest, SoundLoopMode, SoundOutputInfo,
    SoundOutputTarget, SoundPlayRequest, SoundPlayback, SoundResidencyPolicy,
    SoundSoftwareChannelCount, SoundVariationMode, SoundVoiceHandle,
};
use solarity_rendering::WorldCameraFrame;
use solarity_ui::{GlueManager, UiGlueMediaAction};
use thiserror::Error;

use crate::random::BlizzardRand;
use crate::time::RealmClock;

use super::frame_profile::RuntimeFrameProfile;

mod loader;
mod model;
mod movement;
mod output;
mod vocal;
mod zone;

use loader::RuntimeSoundLoader;
pub(super) use movement::UnitSoundContext;

/// Live sound policy and publication of a resolved platform output selection.
pub(super) trait SoundCvarSource {
    fn sound_cvar(&self, name: &str) -> Option<String>;
    fn publish_sound_output(
        &self,
        names: Vec<String>,
        index: usize,
        name: &str,
    ) -> Result<(), solarity_ui::UiScriptError>;
}

impl SoundCvarSource for GlueManager {
    fn sound_cvar(&self, name: &str) -> Option<String> {
        self.cvar_value(name)
    }
    fn publish_sound_output(
        &self,
        names: Vec<String>,
        index: usize,
        name: &str,
    ) -> Result<(), solarity_ui::UiScriptError> {
        self.set_sound_output_devices(names);
        self.set_sound_output_selection(index, name)
    }
}

impl SoundCvarSource for super::world_ui::RuntimeWorldUi {
    fn sound_cvar(&self, name: &str) -> Option<String> {
        self.cvar_value(name)
    }
    fn publish_sound_output(
        &self,
        names: Vec<String>,
        index: usize,
        name: &str,
    ) -> Result<(), solarity_ui::UiScriptError> {
        self.publish_sound_output(names, index, name)
    }
}

/// Failure while applying stock audio policy at the composition root.
#[derive(Debug, Error)]
pub enum RuntimeSoundError {
    /// A resolved device could not be published to the stock sound menu.
    #[error(transparent)]
    Ui(#[from] solarity_ui::UiScriptError),
    /// An output CVar cannot be decoded as the native signed integer.
    #[error("sound output CVar {name} has invalid integer value {value:?}")]
    InvalidOutputOption {
        /// Exact registered output setting.
        name: &'static str,
        /// Unmodified live value.
        value: String,
    },
    /// A movement sound lookup table could not be decoded.
    #[error(transparent)]
    Asset(#[from] solarity_asset::AssetError),
    /// A unit's authored terrain/WMO surface could not be resolved.
    #[error(transparent)]
    MovementSurface(#[from] super::terrain_coordinator::RuntimeMovementRegistrationError),
    /// A terrain liquid query rejected its input.
    #[error(transparent)]
    TerrainLiquid(#[from] solarity_systems::TerrainLiquidError),
    /// A world-model liquid query rejected its input.
    #[error(transparent)]
    WorldModelLiquid(#[from] solarity_systems::WorldModelLiquidError),
    /// The worker archive owner could not be initialized or accessed.
    #[error("sound archive worker is unavailable: {message}")]
    LoaderUnavailable {
        /// Initialization or ownership failure, without a remount retry.
        message: String,
    },
    /// CPU job admission or completion failed.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// A registered stock sound CVar disappeared from the live UI registry.
    #[error("required sound CVar {name} is absent")]
    MissingCVar {
        /// Exact registered name.
        name: &'static str,
    },
    /// A boolean sound CVar no longer contains a finite numeric value.
    #[error("sound CVar {name} has invalid boolean value {value:?}")]
    InvalidBoolean {
        /// Exact registered name.
        name: &'static str,
        /// Unmodified live text.
        value: String,
    },
    /// A gain CVar cannot be represented in stock's zero-to-one domain.
    #[error("sound CVar {name} has invalid gain value {value:?}")]
    InvalidGain {
        /// Exact registered name.
        name: &'static str,
        /// Unmodified live text.
        value: String,
    },
    /// A registered listener offset could not be represented as a finite distance.
    #[error("sound listener offset {name} has invalid value {value:?}")]
    InvalidListenerOffset {
        /// Exact optional CVar name.
        name: &'static str,
        /// Unmodified live text.
        value: String,
    },
    /// `Sound_NumChannels` is not a signed integer that stock can clamp.
    #[error("Sound_NumChannels must be a signed integer, got {value:?}")]
    InvalidVoiceCapacity {
        /// Unmodified live text.
        value: String,
    },
    /// `Sound_MaxCacheableSizeInBytes` is not a signed 32-bit integer.
    #[error("Sound_MaxCacheableSizeInBytes has invalid integer value {value:?}")]
    InvalidMaximumCacheableSize {
        /// Unmodified live text.
        value: String,
    },
    /// `Sound_MaxCacheSizeInBytes` is not a signed 32-bit integer.
    #[error("Sound_MaxCacheSizeInBytes has invalid integer value {value:?}")]
    InvalidMaximumCacheSize {
        /// Unmodified live text.
        value: String,
    },
    /// A main-loop pause exceeded the signed sound-engine tick representation.
    #[error("sound update elapsed time exceeds signed milliseconds")]
    ElapsedTimeCapacity,
    /// Sound DBC, payload, decoder, or SDL output construction failed.
    #[error(transparent)]
    Engine(#[from] SoundEngineError),
    /// An advanced emitter could not satisfy exact lifecycle or spatial policy.
    #[error(transparent)]
    Advanced(#[from] AdvancedSoundServiceError),
}

/// Concrete main-thread owner of stock sound state and resident MCSE objects.
pub(crate) struct RuntimeSoundCoordinator {
    assets: AssetStoreHandle,
    engine: OwnedSoundEngine,
    output: output::RuntimeSoundOutput,
    loader: RuntimeSoundLoader,
    advanced: AdvancedSoundService,
    glue_music: Option<RuntimeGlueVoice>,
    /// 986080 reselects this kit when its admitted track finishes.
    glue_music_repeat: Option<u32>,
    glue_ambience: Option<RuntimeGlueVoice>,
    resident_tile: Option<TerrainTileIndex>,
    staged_emitters: Option<Vec<StagedTerrainEmitter>>,
    last_update: Instant,
    movement_sounds: solarity_asset::MovementSoundCatalog,
    movement_events: std::collections::VecDeque<super::unit_animation::UnitMovementAnimationEvent>,
    movement_loads: Vec<movement::UnitSoundLoad>,
    movement_voices: Vec<SoundVoiceHandle>,
    model_sounds: Vec<model::ModelSound>,
    unit_vocals: Vec<vocal::UnitVocal>,
    world_listener: Option<AdvancedSoundListener>,
    zone: solarity_media::ZoneSoundService,
    zone_references: Option<solarity_asset::AreaSoundReferences>,
    next_zone_references: Option<solarity_asset::AreaSoundReferences>,
    zone_overrides: solarity_asset::ZoneSoundOverrideCatalog,
    chunk_references: Option<solarity_asset::AreaSoundReferences>,
    next_chunk_references: Option<solarity_asset::AreaSoundReferences>,
    state_references: Option<solarity_asset::AreaSoundReferences>,
    next_state_references: Option<solarity_asset::AreaSoundReferences>,
    started: Instant,
    last_fade_update: Instant,
}

impl RuntimeSoundCoordinator {
    /// Focus policy mutes the output bus without rejecting new sound requests.
    pub(crate) fn apply_focus_policy(
        &mut self,
        cvars: &dyn SoundCvarSource,
        focused: bool,
    ) -> Result<(), RuntimeSoundError> {
        let muted = !focused && !boolean(cvars, "Sound_EnableSoundWhenGameIsInBG")?;
        self.engine
            .with_engine_mut(|engine| engine.set_background_muted(muted))?;
        Ok(())
    }

    /// Opens the explicit output after SDL and UI CVar registration exist.
    pub(crate) fn start(
        assets: AssetStoreHandle,
        glue: &GlueManager,
        target: SoundOutputTarget,
        catalog: ArchiveCatalog,
    ) -> Result<Self, RuntimeSoundError> {
        let policy = SoundPolicy::read(glue)?;
        let software_channel_count = software_channel_count(glue)?;
        let localized = glue.localized_text("SYSTEM_DEFAULT")?;
        // 8783B0 falls back when the localization token is absent or empty.
        let default_name = if localized == "SYSTEM_DEFAULT" {
            "System Default".to_owned()
        } else {
            localized
        };
        let output = output::RuntimeSoundOutput::resolve(glue, target, default_name)?;
        let engine = OwnedSoundEngine::load_configured(
            &mut assets.borrow_mut(),
            output.configuration(glue)?,
            software_channel_count,
            policy.settings,
        )?;
        output.publish(glue)?;
        let movement_sounds = solarity_asset::MovementSoundCatalog::load(&mut assets.borrow_mut())?;
        let zone = solarity_media::ZoneSoundService::new(solarity_asset::ZoneSoundCatalog::load(
            &mut assets.borrow_mut(),
        )?);
        let zone_overrides =
            solarity_asset::ZoneSoundOverrideCatalog::load(&mut assets.borrow_mut())?;
        Ok(Self {
            assets,
            engine,
            output,
            loader: RuntimeSoundLoader::new(catalog),
            advanced: AdvancedSoundService::new(),
            glue_music: None,
            glue_music_repeat: None,
            glue_ambience: None,
            resident_tile: None,
            staged_emitters: None,
            last_update: Instant::now(),
            movement_sounds,
            movement_events: std::collections::VecDeque::new(),
            movement_loads: Vec::new(),
            movement_voices: Vec::new(),
            world_listener: None,
            model_sounds: Vec::new(),
            unit_vocals: Vec::new(),
            zone,
            zone_references: None,
            next_zone_references: None,
            zone_overrides,
            chunk_references: None,
            next_chunk_references: None,
            state_references: None,
            next_state_references: None,
            started: Instant::now(),
            last_fade_update: Instant::now(),
        })
    }

    /// Returns the selected device and actual SDL output format.
    #[must_use]
    pub(crate) fn output_info(&self) -> SoundOutputInfo {
        self.engine.with_engine(|engine| engine.output_info())
    }

    /// Returns the exact real software-mix count from `Sound_NumChannels`.
    #[must_use]
    pub(crate) fn software_channel_count(&self) -> usize {
        self.engine
            .with_engine(|engine| engine.software_channel_count())
    }

    /// Returns build 12340's fixed logical FMOD voice pool.
    #[must_use]
    pub(crate) fn engine_voice_capacity(&self) -> usize {
        self.engine.with_engine(|engine| engine.voice_capacity())
    }

    /// Includes current music and ambience admission in complete-transition measurements.
    pub(crate) fn glue_media_ready(&self) -> bool {
        [&self.glue_music, &self.glue_ambience]
            .into_iter()
            .all(|voice| voice.as_ref().is_none_or(|voice| voice.load.is_none()))
    }

    /// Starts movie audio with stock's unsigned-byte volume scale.
    pub(crate) fn start_cinematic_audio(
        &mut self,
        samples: &[i16],
        volume: u32,
    ) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            engine.start_cinematic_audio(samples, volume as f32 / 255.0)
        })?;
        Ok(())
    }

    /// Appends one decoded movie block to the active SDL stream.
    pub(crate) fn queue_cinematic_audio(
        &mut self,
        samples: &[i16],
    ) -> Result<(), RuntimeSoundError> {
        self.engine
            .with_engine_mut(|engine| engine.queue_cinematic_audio(samples))?;
        Ok(())
    }

    /// Returns the output-consumed movie time used by stock as master clock.
    #[must_use]
    pub(crate) fn cinematic_playback_time(&self) -> Option<Duration> {
        self.engine
            .with_engine(|engine| engine.cinematic_playback_time())
    }

    /// Stops and releases movie audio when playback ends or is replaced.
    pub(crate) fn stop_cinematic_audio(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine
            .with_engine_mut(|engine| engine.stop_cinematic_audio())?;
        Ok(())
    }

    /// Applies live Glue sound policy and every ordered Lua audio operation.
    ///
    /// Build 12340 functions `0x009858B0`, `0x00985950`, `0x009859B0`,
    /// `0x009861C0`, and `0x00985FB0` separate UI kits, direct paths, music,
    /// and ambience into their exact channel categories. Script playback
    /// failures do not unwind Lua or terminate the client, so failed individual
    /// actions are diagnosed and the remaining ordered actions still run.
    pub(crate) fn synchronize_glue_media(
        &mut self,
        glue: &GlueManager,
        random: &mut BlizzardRand,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeSoundError> {
        let mut profile = RuntimeFrameProfile::new("Glue audio service");
        let settings = SoundPolicy::read(glue)?.settings;
        profile.mark("read settings");
        self.engine
            .with_engine_mut(|engine| engine.set_settings(settings))?;
        profile.mark("apply settings");
        while let Some(action) = glue.take_media_action() {
            let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS")
                .map(|_| (std::time::Instant::now(), format!("{action:?}")));
            if let Err(error) = self.apply_glue_media_action(action, random, glue) {
                tracing::warn!(%error, "Glue audio action was not played");
            }
            if let Some((started, action)) = timing {
                tracing::info!(
                    action,
                    elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                    "profiled Glue audio action"
                );
            }
        }
        profile.mark("media actions");
        self.poll_loads(cpu)?;
        self.repeat_glue_music(random)?;
        profile.mark("load completions and encoded retirement");
        Ok(())
    }

    /// Advances already selected audio on either side of the Glue/world boundary.
    pub(crate) fn poll_loads(&mut self, cpu: &CpuExecutor) -> Result<(), RuntimeSoundError> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_fade_update);
        self.last_fade_update = now;
        self.engine
            .with_engine_mut(|engine| engine.advance_fades(elapsed))?;
        self.collect_model_sounds()?;
        self.collect_glue_voices()?;
        if let Some(completion) = self
            .engine
            .with_engine_mut(|engine| self.loader.poll(cpu, engine))?
        {
            let playback = match completion.result {
                Ok(playback) => playback,
                Err(error) => {
                    tracing::warn!(%error, "audio resource was not admitted");
                    SoundPlayback::Suppressed
                }
            };
            self.complete_unit_load(completion.handle, playback)?;
            self.complete_model_load(completion.handle, playback)?;
            self.complete_vocal_load(completion.handle, playback)?;
            self.engine.with_engine_mut(|engine| {
                self.zone.complete_load(engine, completion.handle, playback)
            })?;
            for voice in [&mut self.glue_music, &mut self.glue_ambience] {
                if voice
                    .as_ref()
                    .is_some_and(|voice| voice.load == Some(completion.handle))
                    && let Some(pending) = voice.take()
                {
                    *voice = RuntimeGlueVoice::started(pending.identity, playback);
                }
            }
        }
        self.engine
            .with_engine_mut(|engine| engine.collect_unused_encoded());
        Ok(())
    }

    /// Applies one typed GlueXML audio operation without reordering neighbors.
    pub(super) fn apply_glue_media_action(
        &mut self,
        action: UiGlueMediaAction,
        random: &mut BlizzardRand,
        cvars: &dyn SoundCvarSource,
    ) -> Result<(), RuntimeSoundError> {
        self.collect_glue_voices()?;
        match action {
            UiGlueMediaAction::RestartSoundSystem => self.restart_output(cvars)?,
            UiGlueMediaAction::PlaySound(name) => {
                let Some(entry_id) = self
                    .engine
                    .with_engine(|engine| engine.script_sound_entry_id(&name))
                else {
                    tracing::debug!(sound = %name, "Glue UI sound name is absent from stock catalogs");
                    return Ok(());
                };
                let request = SoundPlayRequest::new(
                    entry_id,
                    SoundChannel::SFX,
                    SoundVariationMode::Sequential,
                    SoundLoopMode::Entry,
                    SoundConcurrencyMode::Entry,
                );
                if let Some(load) = self.engine.with_engine_mut(|engine| {
                    engine.begin_load(request, &mut || random.next_u32())
                })? {
                    self.loader.queue(load);
                }
            }
            UiGlueMediaAction::PlaySoundFile(path) => {
                let path = AssetPath::new(path).map_err(SoundEngineError::from)?;
                if let Some(load) = self.engine.with_engine_mut(|engine| {
                    engine.begin_file_load(&path, SoundChannel::SCRIPT_SOUND, SoundLoopMode::Once)
                })? {
                    self.loader.queue(load);
                }
            }
            UiGlueMediaAction::PlayMusic(path) => {
                let path = AssetPath::new(path).map_err(SoundEngineError::from)?;
                let identity = RuntimeGlueVoiceIdentity::File(path.clone());
                if self
                    .glue_music
                    .as_ref()
                    .is_some_and(|voice| voice.identity == identity)
                {
                    return Ok(());
                }
                self.stop_glue_music()?;
                let load = self.engine.with_engine_mut(|engine| {
                    engine.begin_file_load(&path, SoundChannel::SCRIPT_MUSIC, SoundLoopMode::Loop)
                })?;
                self.glue_music = self.queue_glue_voice(identity, load);
            }
            UiGlueMediaAction::PlayGlueMusic(name) | UiGlueMediaAction::PlayCreditsMusic(name) => {
                let Some(entry_id) = self
                    .engine
                    .with_engine(|engine| engine.internal_sound_entry_id(&name))
                else {
                    tracing::debug!(sound = %name, "Glue music name is absent from SoundEntries.dbc");
                    return Ok(());
                };
                let identity = RuntimeGlueVoiceIdentity::SoundEntry(entry_id);
                if self
                    .glue_music
                    .as_ref()
                    .is_some_and(|voice| voice.identity == identity)
                {
                    return Ok(());
                }
                self.stop_glue_music()?;
                self.glue_music_repeat = Some(entry_id);
                self.repeat_glue_music(random)?;
            }
            UiGlueMediaAction::PlayGlueAmbience { name, fade_seconds } => {
                let Some(entry_id) = self
                    .engine
                    .with_engine(|engine| engine.internal_sound_entry_id(&name))
                else {
                    tracing::debug!(sound = %name, "Glue ambience name is absent from SoundEntries.dbc");
                    return Ok(());
                };
                let identity = RuntimeGlueVoiceIdentity::SoundEntry(entry_id);
                if self
                    .glue_ambience
                    .as_ref()
                    .is_some_and(|voice| voice.identity == identity)
                {
                    return Ok(());
                }
                // 985FB0 retires the replaced ambience over three seconds.
                self.fade_glue_ambience(3.0)?;
                let request = SoundPlayRequest::new(
                    entry_id,
                    SoundChannel::AMBIENCE,
                    SoundVariationMode::Random,
                    SoundLoopMode::Loop,
                    SoundConcurrencyMode::Entry,
                );
                let load = self.engine.with_engine_mut(|engine| {
                    engine.begin_load(request, &mut || random.next_u32())
                })?;
                if let Some(load) = &load {
                    // 4C6A40 only overrides the voice fade-in for positive input.
                    let seconds = fade_seconds as f32;
                    if seconds > 0.0 {
                        let mut fade = SoundFade::new(SoundGain::MUTED);
                        fade.retarget_seconds(SoundFadeDirection::In, seconds);
                        self.engine
                            .with_engine_mut(|engine| engine.set_load_fade(load.handle(), fade));
                    }
                }
                self.glue_ambience = self.queue_glue_voice(identity, load);
            }
            UiGlueMediaAction::StopMusic => self.stop_glue_music()?,
            UiGlueMediaAction::StopGlueAmbience => self.fade_glue_ambience(1.0)?,
            UiGlueMediaAction::StopAllSfx { fade_seconds } => {
                self.engine.with_engine_mut(|engine| {
                    engine.fade_channel_out(SoundChannel::SFX, fade_seconds as f32)
                })?;
            }
        }
        Ok(())
    }

    /// Glue's frame callback (986080) reselects an ended kit using the default
    /// random variation and authored loop flag. Looping one initially chosen
    /// file forever would discard the kit's other music variations.
    fn repeat_glue_music(&mut self, random: &mut BlizzardRand) -> Result<(), RuntimeSoundError> {
        if self.glue_music.is_some() {
            return Ok(());
        }
        let Some(entry_id) = self.glue_music_repeat else {
            return Ok(());
        };
        let request = SoundPlayRequest::new(
            entry_id,
            SoundChannel::MUSIC,
            SoundVariationMode::Random,
            SoundLoopMode::Entry,
            SoundConcurrencyMode::Entry,
        );
        let load = self
            .engine
            .with_engine_mut(|engine| engine.begin_load(request, &mut || random.next_u32()))?;
        self.glue_music =
            self.queue_glue_voice(RuntimeGlueVoiceIdentity::SoundEntry(entry_id), load);
        Ok(())
    }

    /// Releases the Glue identity immediately while its engine-owned tail fades.
    /// The script stop wrapper supplies one second (4DC130), replacement three.
    fn fade_glue_ambience(&mut self, seconds: f32) -> Result<(), RuntimeSoundError> {
        if let Some(voice) = self.glue_ambience.take() {
            self.engine
                .with_engine_mut(|engine| -> Result<(), SoundEngineError> {
                    if let Some(load) = voice.load {
                        engine.cancel_load(load);
                    }
                    if let Some(handle) = voice._handle {
                        match engine.voice_fade(handle) {
                            Ok(mut fade) => {
                                fade.retarget_seconds(SoundFadeDirection::Out, seconds);
                                engine.set_voice_fade(handle, fade)?;
                            }
                            Err(SoundEngineError::UnknownVoice) => {}
                            Err(error) => return Err(error),
                        }
                    }
                    Ok(())
                })?;
        }
        Ok(())
    }

    /// Releases finished script/Glue identities so later calls can play again.
    fn collect_glue_voices(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine
            .with_engine_mut(|engine| -> Result<(), SoundEngineError> {
                for slot in [&mut self.glue_music, &mut self.glue_ambience] {
                    if let Some(voice) = slot.as_ref()
                        && !voice.is_live(engine)?
                        && let Some(voice) = slot.take()
                    {
                        voice.stop(engine)?;
                    }
                }
                Ok(())
            })?;
        Ok(())
    }

    /// Retains same-source suppression while the selected payload is still loading.
    fn queue_glue_voice(
        &mut self,
        identity: RuntimeGlueVoiceIdentity,
        request: Option<SoundLoadRequest>,
    ) -> Option<RuntimeGlueVoice> {
        let request = request?;
        let load = request.handle();
        self.loader.queue(request);
        Some(RuntimeGlueVoice {
            identity,
            load: Some(load),
            _handle: None,
        })
    }

    /// Cancels pending Glue reads and observes worker completion before pool shutdown.
    pub(crate) fn shutdown(&mut self) -> Result<(), RuntimeSoundError> {
        self.clear_unit_vocals()?;
        self.engine
            .with_engine_mut(|engine| self.zone.clear(engine))?;
        self.clear_unit_sounds()?;
        self.clear_model_sounds()?;
        self.stop_glue_music()?;
        self.stop_glue_ambience()?;
        self.engine
            .with_engine_mut(|engine| self.loader.shutdown(engine))
    }

    /// Stops the single process-owned Glue music generation.
    fn stop_glue_music(&mut self) -> Result<(), RuntimeSoundError> {
        self.glue_music_repeat = None;
        if let Some(voice) = self.glue_music.take() {
            self.engine.with_engine_mut(|engine| voice.stop(engine))?;
        }
        Ok(())
    }

    /// Stops the single process-owned Glue ambience generation.
    fn stop_glue_ambience(&mut self) -> Result<(), RuntimeSoundError> {
        if let Some(voice) = self.glue_ambience.take() {
            self.engine.with_engine_mut(|engine| voice.stop(engine))?;
        }
        Ok(())
    }

    /// Stages one newly resident ADT's MCSE records in authored chunk order.
    pub(crate) fn stage_terrain_tile(&mut self, tile: &DecodedTerrainTile) {
        if self.resident_tile == Some(tile.index()) {
            return;
        }
        self.resident_tile = Some(tile.index());
        self.staged_emitters = Some(
            tile.chunks()
                .iter()
                .flat_map(|chunk| chunk.sound_emitters())
                .copied()
                .map(StagedTerrainEmitter::from)
                .collect(),
        );
        self.last_update = Instant::now();
    }

    /// Stops all resident world sounds and releases their terrain identity.
    pub(crate) fn disconnect(&mut self) -> Result<(), RuntimeSoundError> {
        self.clear_unit_vocals()?;
        self.engine
            .with_engine_mut(|engine| self.zone.clear(engine))?;
        self.zone_references = None;
        self.next_zone_references = None;
        self.chunk_references = None;
        self.next_chunk_references = None;
        self.state_references = None;
        self.next_state_references = None;
        self.world_listener = None;
        self.movement_events.clear();
        self.clear_unit_sounds()?;
        self.clear_model_sounds()?;
        self.engine
            .with_engine_mut(|engine| self.advanced.clear(engine))?;
        self.resident_tile = None;
        self.staged_emitters = None;
        self.last_update = Instant::now();
        self.engine
            .with_engine_mut(|engine| engine.collect_unused_encoded());
        Ok(())
    }

    /// Applies live CVar policy and advances advanced sounds from camera/time.
    pub(crate) fn update(
        &mut self,
        glue: &dyn SoundCvarSource,
        clock: &RealmClock,
        camera: WorldCameraFrame,
        player_position: Option<Vec3>,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        let policy = SoundPolicy::read(glue)?;
        self.engine
            .with_engine_mut(|engine| engine.set_settings(policy.settings))?;

        let listener = if boolean(glue, "Sound_ListenerAtCharacter")?
            && let Some(position) = player_position
        {
            AdvancedSoundListener::at_character(
                camera,
                position,
                listener_offset(glue, "Sound_ListenerBackDist", 2.0)?,
                listener_offset(glue, "Sound_ListenerUpDist", 4.0)?,
            )
            .map_err(SoundEngineError::from)?
        } else {
            AdvancedSoundListener::from_world_camera(camera)
        };
        self.world_listener = Some(listener);
        self.engine
            .with_engine_mut(|engine| engine.update_listener(listener))?;
        self.advance_world_sounds(glue, clock, listener, random)
    }

    /// Keeps music selection and emitter timers alive while Vulkan is minimized.
    pub(crate) fn update_suspended_world(
        &mut self,
        cvars: &dyn SoundCvarSource,
        clock: &RealmClock,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        let settings = SoundPolicy::read(cvars)?.settings;
        self.engine
            .with_engine_mut(|engine| engine.set_settings(settings))?;
        if let Some(listener) = self.world_listener {
            self.advance_world_sounds(cvars, clock, listener, random)?;
        }
        Ok(())
    }

    fn advance_world_sounds(
        &mut self,
        glue: &dyn SoundCvarSource,
        clock: &RealmClock,
        listener: AdvancedSoundListener,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        self.update_zone(glue, clock, random)?;
        if let Some(emitters) = self.staged_emitters.take() {
            self.engine
                .with_engine_mut(|engine| self.advanced.clear(engine))?;
            for emitter in emitters {
                self.engine.with_engine_mut(|engine| {
                    self.advanced.create(
                        &mut self.assets.borrow_mut(),
                        engine,
                        AdvancedSoundCreateRequest::new(
                            emitter.advanced_sound_entry_id,
                            emitter.position,
                            emitter.cone_orientation,
                        ),
                        listener,
                        &mut || random.next_u32(),
                    )
                })?;
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;
        let elapsed_milliseconds = i32::try_from(elapsed.as_millis())
            .map_err(|_source| RuntimeSoundError::ElapsedTimeCapacity)?;
        self.engine.with_engine_mut(|engine| {
            self.advanced.update(
                &mut self.assets.borrow_mut(),
                engine,
                elapsed_milliseconds,
                clock.day_milliseconds(),
                listener,
                &mut || random.next_u32(),
            )
        })?;
        self.engine
            .with_engine_mut(|engine| engine.collect_unused_encoded());
        Ok(())
    }
}

/// Copy of one decoded MCSE record retained across the frame boundary.
#[derive(Clone, Copy)]
struct StagedTerrainEmitter {
    advanced_sound_entry_id: u32,
    position: Vec3,
    cone_orientation: Vec3,
}

/// Stable identity and backend generation for one continuous Glue voice.
struct RuntimeGlueVoice {
    identity: RuntimeGlueVoiceIdentity,
    load: Option<SoundLoadHandle>,
    _handle: Option<SoundVoiceHandle>,
}

impl RuntimeGlueVoice {
    /// Finished or stolen script voices must stop suppressing zone music.
    fn is_live(&self, engine: &solarity_media::SoundEngine<'_>) -> Result<bool, SoundEngineError> {
        if let Some(load) = self.load {
            return Ok(engine.is_load_pending(load));
        }
        let Some(voice) = self._handle else {
            return Ok(false);
        };
        match engine.voice_state(voice) {
            Ok(
                solarity_media::SoundVoiceState::Playing | solarity_media::SoundVoiceState::Paused,
            ) => Ok(true),
            Ok(solarity_media::SoundVoiceState::Stopped) | Err(SoundEngineError::UnknownVoice) => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Releases only this script/Glue generation, preserving other channel owners.
    fn stop(self, engine: &mut solarity_media::SoundEngine<'_>) -> Result<(), SoundEngineError> {
        if let Some(load) = self.load {
            engine.cancel_load(load);
        }
        if let Some(voice) = self._handle {
            match engine.stop(voice) {
                Ok(()) | Err(SoundEngineError::UnknownVoice) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Retains only playback generations actually admitted by live CVar policy.
    fn started(identity: RuntimeGlueVoiceIdentity, playback: SoundPlayback) -> Option<Self> {
        match playback {
            SoundPlayback::Started(handle) => Some(Self {
                identity,
                load: None,
                _handle: Some(handle),
            }),
            SoundPlayback::Suppressed => None,
        }
    }
}

/// Exact source namespace used for same-request suppression.
#[derive(Eq, PartialEq)]
enum RuntimeGlueVoiceIdentity {
    /// DBC-backed kit identity.
    SoundEntry(u32),
    /// Direct archive resource identity.
    File(AssetPath),
}

impl From<solarity_asset::TerrainSoundEmitter> for StagedTerrainEmitter {
    fn from(emitter: solarity_asset::TerrainSoundEmitter) -> Self {
        Self {
            advanced_sound_entry_id: emitter.advanced_sound_entry_id(),
            position: Vec3::from_array(emitter.position()),
            cone_orientation: Vec3::from_array(emitter.cone_orientation()),
        }
    }
}

/// One complete live CVar snapshot applied atomically to the engine.
struct SoundPolicy {
    settings: SoundEngineSettings,
}

impl SoundPolicy {
    fn read(glue: &dyn SoundCvarSource) -> Result<Self, RuntimeSoundError> {
        let maximum_cacheable_size_text = cvar(glue, "Sound_MaxCacheableSizeInBytes")?;
        // The stock CVar is a signed integer, but SoundEngine.cpp reads its raw
        // 32-bit word and then applies an unsigned two-megabyte ceiling.
        let configured_maximum_cacheable_size = maximum_cacheable_size_text
            .parse::<i32>()
            .map(|value| value as u32)
            .map_err(|_source| RuntimeSoundError::InvalidMaximumCacheableSize {
                value: maximum_cacheable_size_text,
            })?;
        let maximum_cache_size_text = cvar(glue, "Sound_MaxCacheSizeInBytes")?;
        let configured_maximum_cache_size = maximum_cache_size_text
            .parse::<i32>()
            .map(|value| value as u32)
            .map_err(|_source| RuntimeSoundError::InvalidMaximumCacheSize {
                value: maximum_cache_size_text,
            })?;
        Ok(Self {
            settings: SoundEngineSettings::new(
                boolean(glue, "Sound_EnableAllSound")?,
                gain(glue, "Sound_MasterVolume")?,
                SoundCategorySettings::new(
                    boolean(glue, "Sound_EnableSFX")?,
                    gain(glue, "Sound_SFXVolume")?,
                ),
                SoundCategorySettings::new(
                    boolean(glue, "Sound_EnableMusic")?,
                    gain(glue, "Sound_MusicVolume")?,
                ),
                SoundCategorySettings::new(
                    boolean(glue, "Sound_EnableAmbience")?,
                    gain(glue, "Sound_AmbienceVolume")?,
                ),
                SoundResidencyPolicy::new(
                    configured_maximum_cacheable_size,
                    configured_maximum_cache_size,
                ),
            ),
        })
    }
}

/// Applies the signed initialization clamp recovered from SoundEngine.cpp.
fn software_channel_count(
    glue: &dyn SoundCvarSource,
) -> Result<SoundSoftwareChannelCount, RuntimeSoundError> {
    let text = cvar(glue, "Sound_NumChannels")?;
    let configured =
        text.parse::<i32>()
            .map_err(|_source| RuntimeSoundError::InvalidVoiceCapacity {
                value: text.clone(),
            })?;
    Ok(SoundSoftwareChannelCount::new(configured))
}

fn cvar(glue: &dyn SoundCvarSource, name: &'static str) -> Result<String, RuntimeSoundError> {
    glue.sound_cvar(name)
        .ok_or(RuntimeSoundError::MissingCVar { name })
}

fn boolean(glue: &dyn SoundCvarSource, name: &'static str) -> Result<bool, RuntimeSoundError> {
    let value = cvar(glue, name)?;
    value
        .parse::<f32>()
        .ok()
        .filter(|parsed| parsed.is_finite())
        .map(|parsed| parsed != 0.0)
        .ok_or(RuntimeSoundError::InvalidBoolean { name, value })
}

fn gain(glue: &dyn SoundCvarSource, name: &'static str) -> Result<SoundGain, RuntimeSoundError> {
    let value = cvar(glue, name)?;
    let parsed = value
        .parse::<f32>()
        .map_err(|_source| RuntimeSoundError::InvalidGain {
            name,
            value: value.clone(),
        })?;
    SoundGain::new(parsed).map_err(|_source| RuntimeSoundError::InvalidGain { name, value })
}

/// 4FA5F0 uses offsets 2/4 when these optional developer CVars are unregistered.
fn listener_offset(
    source: &dyn SoundCvarSource,
    name: &'static str,
    default: f32,
) -> Result<f32, RuntimeSoundError> {
    let Some(value) = source.sound_cvar(name) else {
        return Ok(default);
    };
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or(RuntimeSoundError::InvalidListenerOffset { name, value })
}
