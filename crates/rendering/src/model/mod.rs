//! GPU resources and draw preparation for M2 and WMO models.
//!
//! `M2Model.cpp`, `M2Scene.cpp`, `MapObj.cpp`, and `ModelBlob.cpp` evidence the
//! stock model path. Binary parsing remains in `solarity-asset`.

mod character_component;
mod character_model_base;
mod component_utils;
mod m2_scene;

pub use character_component::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterItemAttachment, CharacterRangedHand, CharacterTextureComposeError,
    CharacterTexturePlan, CharacterTexturePlanError, CharacterWeaponPose, CharacterWeaponState,
};
