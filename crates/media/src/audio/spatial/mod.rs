//! Zone and world-space sound selection evidenced by `SoundInterface2ZoneSounds.cpp`.

mod catalog;
mod liquid;
mod sound_interface2_zone_sounds;

pub use catalog::{ResolvedSpatialSound, SpatialSoundCatalog, SpatialSoundError};
pub use liquid::{LiquidSoundCatalog, LiquidSoundError, ResolvedLiquidSound};
