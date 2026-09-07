//! Sound loading, playback, mixing, spatialization, DSP, and zone ambience.
//!
//! The boundary is evidenced by `SoundEngine.cpp`, `SoundInterface2.cpp`,
//! `SoundInterface2DSP.cpp`, `SoundInterface2Internal.cpp`, and
//! `SoundInterface2ZoneSounds.cpp`. SDL3_mixer supplies playback primitives;
//! this module owns stock-facing policy and asset integration.

mod backend;
mod cache;
mod codec;
mod dsp;
mod emitter;
mod engine;
mod selection;
mod spatial;

mod types;

pub use backend::{
    SoundBackend, SoundBackendError, SoundBackendPlayback, SoundOutput, SoundOutputConfiguration,
    SoundOutputDevice, SoundOutputDeviceId, SoundOutputInfo, SoundOutputQuality, SoundOutputTarget,
    SoundSpatialPosition, SoundVoiceHandle, SoundVoicePriority, SoundVoiceState,
};
pub use cache::{EncodedSound, SoundCache};
pub use codec::{
    DecodedSoundHandle, DecodedSoundInfo, SoundDecodeError, SoundDecodeMode, SoundDecoder,
};
pub use engine::{
    AdvancedSoundCreateRequest, AdvancedSoundDirective, AdvancedSoundDucking,
    AdvancedSoundInstanceId, AdvancedSoundLifecycle, AdvancedSoundListener,
    AdvancedSoundProperties, AdvancedSoundService, AdvancedSoundServiceError,
    AdvancedSoundSpatialError, AdvancedSoundSpatialMix, AdvancedSoundUpdateReport,
    AdvancedSoundUsage, AdvancedSoundUsageError, OwnedSoundEngine, SoundCategory,
    SoundCategorySettings, SoundChannel, SoundChannelError, SoundConcurrencyMode, SoundEngine,
    SoundEngineError, SoundEngineSettings, SoundFade, SoundFadeDirection, SoundGain,
    SoundGainError, SoundLoadHandle, SoundLoadRequest, SoundLoopMode, SoundPlayRequest,
    SoundPlayback, SoundResidencyPolicy, SoundSoftwareChannelCount,
};
pub use selection::{SoundVariationMode, SoundVariationSelector};
pub use spatial::{
    LiquidSoundCatalog, LiquidSoundError, ResolvedLiquidSound, ResolvedSpatialSound,
    SpatialSoundCatalog, SpatialSoundError, ZoneMusicCue, ZoneMusicSelection, ZoneSoundFrame,
    ZoneSoundLayer, ZoneSoundLocationIds, ZoneSoundOptions, ZoneSoundService, ZoneSoundState,
    ZoneSoundTimeOfDay, resolve_world_state_zone_sounds, resolve_zone_sound_references,
    world_chunk_sound_key,
};
