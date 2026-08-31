//! Audio playback, cinematic decoding, and media timing boundaries.
//!
//! SDL3_mixer owns stock WAV and MP3 playback. FFmpeg owns the stock AVI
//! demuxing, MPEG-4 Part 2 video decoding, MP3 audio decoding, audio resampling,
//! and CPU-side video frame conversion paths.

mod audio;
mod cinematic;
mod voice;

pub use audio::{
    AdvancedSoundDirective, AdvancedSoundDucking, AdvancedSoundInstanceId, AdvancedSoundLifecycle,
    AdvancedSoundListener, AdvancedSoundProperties, AdvancedSoundSpatialError,
    AdvancedSoundSpatialMix, AdvancedSoundUsage, AdvancedSoundUsageError, DecodedSoundHandle,
    DecodedSoundInfo, EncodedSound, LiquidSoundCatalog, LiquidSoundError, ResolvedLiquidSound,
    ResolvedSpatialSound, SoundBackend, SoundBackendError, SoundCache, SoundCategory,
    SoundCategorySettings, SoundDecodeError, SoundDecodeMode, SoundDecoder, SoundEngine,
    SoundEngineError, SoundEngineSettings, SoundGain, SoundGainError, SoundOutput, SoundOutputInfo,
    SoundOutputTarget, SoundPlayRequest, SoundPlayback, SoundSpatialPosition, SoundVariationMode,
    SoundVariationSelector, SoundVoiceHandle, SoundVoiceState, SpatialSoundCatalog,
    SpatialSoundError,
};
