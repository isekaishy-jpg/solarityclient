//! Zone and world-space sound selection evidenced by `SoundInterface2ZoneSounds.cpp`.

mod catalog;
mod liquid;
mod sound_interface2_zone_sounds;
mod zone_overrides;
mod zone_playback;

pub use zone_overrides::{
    ZoneSoundLocationIds, resolve_world_state_zone_sounds, world_chunk_sound_key,
};
pub use zone_playback::ZoneSoundService;

pub use catalog::{ResolvedSpatialSound, SpatialSoundCatalog, SpatialSoundError};
pub use liquid::{LiquidSoundCatalog, LiquidSoundError, ResolvedLiquidSound};
pub use sound_interface2_zone_sounds::{
    ZoneMusicCue, ZoneMusicSelection, ZoneSoundFrame, ZoneSoundLayer, ZoneSoundOptions,
    ZoneSoundState, ZoneSoundTimeOfDay, resolve_zone_sound_references,
};
