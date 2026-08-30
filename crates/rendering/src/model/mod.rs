//! GPU resources and draw preparation for M2 and WMO models.
//!
//! `M2Model.cpp`, `M2Scene.cpp`, `MapObj.cpp`, and `ModelBlob.cpp` evidence the
//! stock model path. Binary parsing remains in `solarity-asset`.

mod character_component;
mod character_model_base;
mod component_utils;
mod m2_animation;
mod m2_scene;

pub use character_component::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterGeosetContext, CharacterGeosetPlan, CharacterGeosetPlanError, CharacterItemAttachment,
    CharacterRangedHand, CharacterTabardMode, CharacterTextureComposeError, CharacterTexturePlan,
    CharacterTexturePlanError, CharacterWeaponPose, CharacterWeaponState,
};
pub use m2_animation::{M2AnimationClock, M2BonePose, M2BonePoseError};
pub use m2_scene::{
    M2DrawCall, M2DrawPushConstants, M2LocalLightState, M2MaterialUniform, M2MeshPlan,
    M2MeshPlanError, M2RenderVertex, M2SceneUniform, M2ShadowMatrix, M2ShadowState,
    M2TextureBinding,
};
