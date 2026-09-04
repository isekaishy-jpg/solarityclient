//! Audio playback, cinematic decoding, and media timing boundaries.
//!
//! SDL3_mixer owns stock WAV and MP3 playback. FFmpeg owns the stock AVI
//! demuxing, MPEG-4 Part 2 video decoding, MP3 audio decoding, audio resampling,
//! and CPU-side video frame conversion paths.

mod audio;
mod cinematic;

pub use cinematic::{CinematicAudioFrame, CinematicDecoder, CinematicError, CinematicVideoFrame};
mod voice;

pub use audio::{
    AdvancedSoundCreateRequest, AdvancedSoundDirective, AdvancedSoundDucking,
    AdvancedSoundInstanceId, AdvancedSoundLifecycle, AdvancedSoundListener,
    AdvancedSoundProperties, AdvancedSoundService, AdvancedSoundServiceError,
    AdvancedSoundSpatialError, AdvancedSoundSpatialMix, AdvancedSoundUpdateReport,
    AdvancedSoundUsage, AdvancedSoundUsageError, DecodedSoundHandle, DecodedSoundInfo,
    EncodedSound, LiquidSoundCatalog, LiquidSoundError, OwnedSoundEngine, ResolvedLiquidSound,
    ResolvedSpatialSound, SoundBackend, SoundBackendError, SoundBackendPlayback, SoundCache,
    SoundCategory, SoundCategorySettings, SoundChannel, SoundChannelError, SoundConcurrencyMode,
    SoundDecodeError, SoundDecodeMode, SoundDecoder, SoundEngine, SoundEngineError,
    SoundEngineSettings, SoundGain, SoundGainError, SoundLoadHandle, SoundLoadRequest,
    SoundLoopMode, SoundOutput, SoundOutputInfo, SoundOutputTarget, SoundPlayRequest,
    SoundPlayback, SoundResidencyPolicy, SoundSoftwareChannelCount, SoundSpatialPosition,
    SoundVariationMode, SoundVariationSelector, SoundVoiceHandle, SoundVoicePriority,
    SoundVoiceState, SpatialSoundCatalog, SpatialSoundError,
};
