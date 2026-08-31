//! Audio playback, cinematic decoding, and media timing boundaries.
//!
//! SDL3_mixer owns stock WAV and MP3 playback. FFmpeg owns the stock AVI
//! demuxing, MPEG-4 Part 2 video decoding, MP3 audio decoding, audio resampling,
//! and CPU-side video frame conversion paths.

mod audio;
mod cinematic;
mod voice;

pub use audio::{
    AdvancedSoundProperties, DecodedSoundHandle, DecodedSoundInfo, EncodedSound,
    ResolvedSpatialSound, SoundBackend, SoundBackendError, SoundCache, SoundCategory,
    SoundCategorySettings, SoundDecodeError, SoundDecodeMode, SoundDecoder, SoundEngine,
    SoundEngineError, SoundEngineSettings, SoundGain, SoundGainError, SoundOutput, SoundOutputInfo,
    SoundOutputTarget, SoundPlayRequest, SoundPlayback, SoundVariationSelector, SoundVoiceHandle,
    SoundVoiceState, SpatialSoundCatalog, SpatialSoundError,
};
