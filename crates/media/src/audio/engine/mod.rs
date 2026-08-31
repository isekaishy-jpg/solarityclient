//! Stock sound-engine lifecycle, channel, and advanced-kit orchestration boundary.

mod sound_engine;
mod sound_interface2;
mod sound_interface2_advanced_kit_ducking;
mod sound_interface2_advanced_kit_lifecycle;
mod sound_interface2_advanced_kit_properties;
mod sound_interface2_advanced_kit_service;
mod sound_interface2_advanced_kit_spatial;
mod sound_interface2_internal;
mod status;
mod types;

pub use sound_engine::SoundEngine;
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
pub use status::{SoundCategoryError, SoundEngineError, SoundGainError};
pub use types::{
    SoundCategory, SoundCategorySettings, SoundEngineSettings, SoundGain, SoundPlayRequest,
    SoundPlayback,
};
