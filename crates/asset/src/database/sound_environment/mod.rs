//! Stock environment-sound tables kept distinct from ADT MCSE emitters.

mod liquid_type;
mod overrides;
mod sound_emitter;
mod world_model_area;
mod zone;

pub use liquid_type::{LiquidTypeCatalog, LiquidTypeDefinition};
pub use overrides::{WorldChunkSoundKey, WorldStateZoneSound, ZoneSoundOverrideCatalog};
pub use sound_emitter::{SoundEmitterCatalog, SoundEmitterDefinition};
pub use world_model_area::{WorldModelAreaCatalog, WorldModelAreaDefinition, WorldModelAreaKey};
pub use zone::{
    AreaSoundReferences, SoundAmbienceDefinition, ZoneIntroMusicDefinition, ZoneMusicDefinition,
    ZoneSoundCatalog,
};
