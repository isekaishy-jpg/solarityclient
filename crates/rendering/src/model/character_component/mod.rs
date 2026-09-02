//! Stock character-model texture bindings and atlas preparation.
//!
//! Build 12340 constructs one dynamic body texture from `CharSections.dbc`,
//! then independently replaces the M2 hair and extra-skin texture slots.

mod atlas;
mod attachment;
mod composer;
mod equipment;
mod geoset;
mod item_visual;
mod status;
mod types;

pub use atlas::CharacterTexturePlan;
pub use attachment::{
    CharacterAttachmentPlan, CharacterAttachmentPoint, CharacterItemAttachment,
    CharacterSelectionQuiver, CharacterWeaponState,
};
pub use equipment::CharacterEquipmentItem;
pub use geoset::{
    CharacterGeosetContext, CharacterGeosetPlan, CharacterTabardMode, CreatureGeosetPlan,
};
pub use item_visual::{CharacterItemVisualEffect, CharacterItemVisualPlan};
pub use status::{
    CharacterAttachmentPlanError, CharacterGeosetPlanError, CharacterTextureComposeError,
    CharacterTexturePlanError,
};
pub use types::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture,
};
