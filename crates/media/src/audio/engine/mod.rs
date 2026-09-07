//! Stock sound-engine lifecycle, channel, and advanced-kit orchestration boundary.

mod fade;
mod sound_engine;
mod sound_engine_owner;
mod sound_interface2;
mod sound_interface2_advanced_kit_ducking;
mod sound_interface2_advanced_kit_lifecycle;
mod sound_interface2_advanced_kit_properties;
mod sound_interface2_advanced_kit_service;
mod sound_interface2_advanced_kit_spatial;
mod sound_interface2_internal;
mod status;
mod types;

pub use fade::{SoundFade, SoundFadeDirection};
pub use sound_engine::{SoundEngine, SoundLoadHandle, SoundLoadRequest};
pub use sound_engine_owner::OwnedSoundEngine;
pub use sound_interface2_advanced_kit_ducking::{AdvancedSoundDucking, AdvancedSoundInstanceId};
pub use sound_interface2_advanced_kit_lifecycle::{
    AdvancedSoundDirective, AdvancedSoundLifecycle, AdvancedSoundUsage, AdvancedSoundUsageError,
};
pub use sound_interface2_advanced_kit_properties::AdvancedSoundProperties;
pub use sound_interface2_advanced_kit_service::{
    AdvancedSoundCreateRequest, AdvancedSoundService, AdvancedSoundServiceError,
    AdvancedSoundUpdateReport,
};
pub use sound_interface2_advanced_kit_spatial::{
    AdvancedSoundListener, AdvancedSoundSpatialError, AdvancedSoundSpatialMix,
};
pub use sound_interface2_internal::{SoundConcurrencyMode, SoundLoopMode, SoundResidencyPolicy};
pub use status::{SoundChannelError, SoundEngineError, SoundGainError};
pub use types::{
    SoundCategory, SoundCategorySettings, SoundChannel, SoundEngineSettings, SoundGain,
    SoundPlayRequest, SoundPlayback, SoundSoftwareChannelCount,
};
