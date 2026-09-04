//! Main-thread sound output, live CVar policy, and terrain-emitter ownership.

use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStoreHandle, DecodedTerrainTile, TerrainTileIndex,
};
use solarity_cpu::{CpuError, CpuExecutor};
use solarity_media::{
    AdvancedSoundCreateRequest, AdvancedSoundListener, AdvancedSoundService,
    AdvancedSoundServiceError, OwnedSoundEngine, SoundCategory, SoundCategorySettings,
    SoundChannel, SoundConcurrencyMode, SoundEngineError, SoundEngineSettings, SoundGain,
    SoundLoadHandle, SoundLoadRequest, SoundLoopMode, SoundOutputInfo, SoundOutputTarget,
    SoundPlayRequest, SoundPlayback, SoundResidencyPolicy, SoundSoftwareChannelCount,
    SoundVariationMode, SoundVoiceHandle,
};
use solarity_rendering::WorldCameraFrame;
use solarity_ui::{GlueManager, UiGlueMediaAction};
use thiserror::Error;

use crate::random::BlizzardRand;
use crate::time::RealmClock;

use super::terrain_frame::RuntimeM2Event;

mod loader;

use loader::RuntimeSoundLoader;

const M2_ONE_SHOT_SOUND_IDENTIFIERS: [[u8; 4]; 3] = [*b"$SND", *b"$CSD", *b"$DSO"];

/// Failure while applying stock audio policy at the composition root.
#[derive(Debug, Error)]
pub enum RuntimeSoundError {
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
    loader: RuntimeSoundLoader,
    advanced: AdvancedSoundService,
    glue_music: Option<RuntimeGlueVoice>,
    glue_ambience: Option<RuntimeGlueVoice>,
    resident_tile: Option<TerrainTileIndex>,
    staged_emitters: Option<Vec<StagedTerrainEmitter>>,
    last_update: Instant,
}

impl RuntimeSoundCoordinator {
    /// Opens the explicit output after SDL and UI CVar registration exist.
    pub(crate) fn start(
        assets: AssetStoreHandle,
        glue: &GlueManager,
        target: SoundOutputTarget,
        catalog: ArchiveCatalog,
    ) -> Result<Self, RuntimeSoundError> {
        let policy = SoundPolicy::read(glue)?;
        let software_channel_count = software_channel_count(glue)?;
        let engine = OwnedSoundEngine::load(
            &mut assets.borrow_mut(),
            target,
            software_channel_count,
            policy.settings,
        )?;
        Ok(Self {
            assets,
            engine,
            loader: RuntimeSoundLoader::new(catalog),
            advanced: AdvancedSoundService::new(),
            glue_music: None,
            glue_ambience: None,
            resident_tile: None,
            staged_emitters: None,
            last_update: Instant::now(),
        })
    }

    /// Returns the selected device and actual SDL output format.
    #[must_use]
    pub(crate) fn output_info(&self) -> SoundOutputInfo {
        self.engine.output_info()
    }

    /// Returns the exact real software-mix count from `Sound_NumChannels`.
    #[must_use]
    pub(crate) fn software_channel_count(&self) -> usize {
        self.engine.software_channel_count()
    }

    /// Returns build 12340's fixed logical FMOD voice pool.
    #[must_use]
    pub(crate) fn engine_voice_capacity(&self) -> usize {
        self.engine.voice_capacity()
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
        self.engine
            .start_cinematic_audio(samples, volume as f32 / 255.0)?;
        Ok(())
    }

    /// Appends one decoded movie block to the active SDL stream.
    pub(crate) fn queue_cinematic_audio(
        &mut self,
        samples: &[i16],
    ) -> Result<(), RuntimeSoundError> {
        self.engine.queue_cinematic_audio(samples)?;
        Ok(())
    }

    /// Returns the output-consumed movie time used by stock as master clock.
    #[must_use]
    pub(crate) fn cinematic_playback_time(&self) -> Option<Duration> {
        self.engine.cinematic_playback_time()
    }

    /// Stops and releases movie audio when playback ends or is replaced.
    pub(crate) fn stop_cinematic_audio(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.stop_cinematic_audio()?;
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
        self.engine
            .set_settings(SoundPolicy::read(glue)?.settings)?;
        while let Some(action) = glue.take_media_action() {
            let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS")
                .map(|_| (std::time::Instant::now(), format!("{action:?}")));
            if let Err(error) = self.apply_glue_media_action(action, random) {
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
        if let Some(completion) = self.loader.poll(cpu, &self.engine)? {
            let playback = match completion.result {
                Ok(encoded) => self.engine.complete_load(completion.handle, &encoded),
                Err(error) => {
                    self.engine.cancel_load(completion.handle);
                    tracing::warn!(%error, "Glue audio payload was not loaded");
                    Ok(SoundPlayback::Suppressed)
                }
            };
            let playback = match playback {
                Ok(playback) => playback,
                Err(error) => {
                    tracing::warn!(%error, "Glue audio resource was not admitted");
                    SoundPlayback::Suppressed
                }
            };
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
        self.engine.collect_unused_encoded();
        Ok(())
    }

    /// Applies one typed GlueXML audio operation without reordering neighbors.
    fn apply_glue_media_action(
        &mut self,
        action: UiGlueMediaAction,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        match action {
            UiGlueMediaAction::PlaySound(name) => {
                let Some(entry_id) = self.engine.script_sound_entry_id(&name) else {
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
                self.engine
                    .play(&mut self.assets.borrow_mut(), request, &mut || {
                        random.next_u32()
                    })?;
            }
            UiGlueMediaAction::PlaySoundFile(path) => {
                let path = AssetPath::new(path).map_err(SoundEngineError::from)?;
                self.engine.play_file(
                    &mut self.assets.borrow_mut(),
                    &path,
                    SoundChannel::SCRIPT_SOUND,
                    SoundLoopMode::Once,
                )?;
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
                let load = self.engine.begin_file_load(
                    &path,
                    SoundChannel::SCRIPT_MUSIC,
                    SoundLoopMode::Loop,
                )?;
                self.glue_music = self.queue_glue_voice(identity, load);
            }
            UiGlueMediaAction::PlayGlueMusic(name) | UiGlueMediaAction::PlayCreditsMusic(name) => {
                let Some(entry_id) = self.engine.internal_sound_entry_id(&name) else {
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
                let request = SoundPlayRequest::new(
                    entry_id,
                    SoundChannel::MUSIC,
                    SoundVariationMode::Sequential,
                    SoundLoopMode::Loop,
                    SoundConcurrencyMode::Concurrent,
                );
                let load = self.engine.begin_load(request, &mut || random.next_u32())?;
                self.glue_music = self.queue_glue_voice(identity, load);
            }
            UiGlueMediaAction::PlayGlueAmbience {
                name,
                fade_seconds: _fade_seconds,
            } => {
                let Some(entry_id) = self.engine.internal_sound_entry_id(&name) else {
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
                self.stop_glue_ambience()?;
                let request = SoundPlayRequest::new(
                    entry_id,
                    SoundChannel::AMBIENCE,
                    SoundVariationMode::Sequential,
                    SoundLoopMode::Loop,
                    SoundConcurrencyMode::Concurrent,
                );
                let load = self.engine.begin_load(request, &mut || random.next_u32())?;
                self.glue_ambience = self.queue_glue_voice(identity, load);
            }
            UiGlueMediaAction::StopMusic => self.stop_glue_music()?,
            UiGlueMediaAction::StopGlueAmbience => self.stop_glue_ambience()?,
            UiGlueMediaAction::StopAllSfx {
                fade_seconds: _fade_seconds,
            } => {
                self.engine.stop_category(SoundCategory::Sfx)?;
            }
        }
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
        self.stop_glue_music()?;
        self.stop_glue_ambience()?;
        self.loader.shutdown(&mut self.engine)
    }

    /// Stops the single process-owned Glue music generation.
    fn stop_glue_music(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.stop_category(SoundCategory::Music)?;
        self.glue_music = None;
        Ok(())
    }

    /// Stops the single process-owned Glue ambience generation.
    fn stop_glue_ambience(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.stop_category(SoundCategory::Ambience)?;
        self.glue_ambience = None;
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
        self.advanced.clear(&mut self.engine)?;
        self.resident_tile = None;
        self.staged_emitters = None;
        self.last_update = Instant::now();
        self.engine.collect_unused_encoded();
        Ok(())
    }

    /// Applies live CVar policy and advances advanced sounds from camera/time.
    pub(crate) fn update(
        &mut self,
        glue: &GlueManager,
        clock: &RealmClock,
        camera: WorldCameraFrame,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        let policy = SoundPolicy::read(glue)?;
        self.engine.set_settings(policy.settings)?;

        let listener = AdvancedSoundListener::from_world_camera(camera);
        if let Some(emitters) = self.staged_emitters.take() {
            self.advanced.clear(&mut self.engine)?;
            for emitter in emitters {
                self.advanced.create(
                    &mut self.assets.borrow_mut(),
                    &mut self.engine,
                    AdvancedSoundCreateRequest::new(
                        emitter.advanced_sound_entry_id,
                        emitter.position,
                        emitter.cone_orientation,
                    ),
                    listener,
                    &mut || random.next_u32(),
                )?;
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;
        let elapsed_milliseconds = i32::try_from(elapsed.as_millis())
            .map_err(|_source| RuntimeSoundError::ElapsedTimeCapacity)?;
        self.advanced.update(
            &mut self.assets.borrow_mut(),
            &mut self.engine,
            elapsed_milliseconds,
            clock.day_milliseconds(),
            listener,
            &mut || random.next_u32(),
        )?;
        self.engine.collect_unused_encoded();
        Ok(())
    }

    /// Dispatches the model callback families routed to `playSoundEntryAt`.
    ///
    /// `$DSL` and every non-audio callback remain with their future owning
    /// subsystem. In particular, a loop callback cannot be represented as a
    /// one-shot without losing the stock stop/update lifecycle.
    pub(crate) fn play_m2_events(
        &mut self,
        events: &[RuntimeM2Event],
        camera: WorldCameraFrame,
        random: &mut BlizzardRand,
    ) -> Result<usize, RuntimeSoundError> {
        let listener = AdvancedSoundListener::from_world_camera(camera);
        let mut dispatched = 0;
        for event in events {
            if !M2_ONE_SHOT_SOUND_IDENTIFIERS.contains(&event.identifier()) {
                continue;
            }
            let request = SoundPlayRequest::new(
                event.data(),
                SoundChannel::SFX,
                SoundVariationMode::Sequential,
                SoundLoopMode::Once,
                SoundConcurrencyMode::Entry,
            );
            self.engine.play_positioned(
                &mut self.assets.borrow_mut(),
                request,
                listener,
                event.position(),
                &mut || random.next_u32(),
            )?;
            tracing::trace!(
                identifier = ?event.identifier(),
                sound_entry_id = event.data(),
                owner_guid = ?event.owner_guid(),
                "dispatched M2 positional sound callback"
            );
            dispatched += 1;
        }
        Ok(dispatched)
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
    fn read(glue: &GlueManager) -> Result<Self, RuntimeSoundError> {
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
    glue: &GlueManager,
) -> Result<SoundSoftwareChannelCount, RuntimeSoundError> {
    let text = cvar(glue, "Sound_NumChannels")?;
    let configured =
        text.parse::<i32>()
            .map_err(|_source| RuntimeSoundError::InvalidVoiceCapacity {
                value: text.clone(),
            })?;
    Ok(SoundSoftwareChannelCount::new(configured))
}

fn cvar(glue: &GlueManager, name: &'static str) -> Result<String, RuntimeSoundError> {
    glue.cvar_value(name)
        .ok_or(RuntimeSoundError::MissingCVar { name })
}

fn boolean(glue: &GlueManager, name: &'static str) -> Result<bool, RuntimeSoundError> {
    let value = cvar(glue, name)?;
    value
        .parse::<f32>()
        .ok()
        .filter(|parsed| parsed.is_finite())
        .map(|parsed| parsed != 0.0)
        .ok_or(RuntimeSoundError::InvalidBoolean { name, value })
}

fn gain(glue: &GlueManager, name: &'static str) -> Result<SoundGain, RuntimeSoundError> {
    let value = cvar(glue, name)?;
    let parsed = value
        .parse::<f32>()
        .map_err(|_source| RuntimeSoundError::InvalidGain {
            name,
            value: value.clone(),
        })?;
    SoundGain::new(parsed).map_err(|_source| RuntimeSoundError::InvalidGain { name, value })
}
