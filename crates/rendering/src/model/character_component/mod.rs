//! Stock character-model texture bindings and atlas preparation.
//!
//! Build 12340 constructs one dynamic body texture from `CharSections.dbc`,
//! then independently replaces the M2 hair and extra-skin texture slots.

mod atlas;
mod status;
mod types;

pub use atlas::CharacterTexturePlan;
pub use status::CharacterTexturePlanError;
pub use types::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasRect, CharacterAtlasRegion,
};
