//! Main-thread sound output, live CVar policy, and terrain-emitter ownership.

use std::time::Instant;

use glam::Vec3;
use solarity_asset::{AssetStoreHandle, DecodedTerrainTile, TerrainTileIndex};
use solarity_media::{
    AdvancedSoundCreateRequest, AdvancedSoundListener, AdvancedSoundService,
    AdvancedSoundServiceError, OwnedSoundEngine, SoundCategorySettings, SoundEngineError,
    SoundEngineSettings, SoundGain, SoundOutputInfo, SoundOutputTarget, SoundResidencyPolicy,
    SoundSoftwareChannelCount,
};
use solarity_rendering::WorldCameraFrame;
use solarity_ui::GlueManager;
use thiserror::Error;

use crate::random::BlizzardRand;
use crate::time::RealmClock;

/// Failure while applying stock audio policy at the composition root.
#[derive(Debug, Error)]
pub enum RuntimeSoundError {
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
    advanced: AdvancedSoundService,
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
            advanced: AdvancedSoundService::new(),
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
}

/// Copy of one decoded MCSE record retained across the frame boundary.
#[derive(Clone, Copy)]
struct StagedTerrainEmitter {
    advanced_sound_entry_id: u32,
    position: Vec3,
    cone_orientation: Vec3,
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
