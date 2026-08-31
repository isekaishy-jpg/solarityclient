//! Stock sound-engine lifecycle, channel, and advanced-kit orchestration boundary.

mod sound_engine;
mod sound_interface2;
mod sound_interface2_advanced_kit_properties;
mod sound_interface2_internal;
mod status;
mod types;

pub use sound_engine::SoundEngine;
pub use status::{SoundEngineError, SoundGainError};
pub use types::{
    SoundCategory, SoundCategorySettings, SoundEngineSettings, SoundGain, SoundPlayRequest,
    SoundPlayback,
};
