//! GPU resources and draw preparation for M2 and WMO models.
//!
//! `M2Model.cpp`, `M2Scene.cpp`, `MapObj.cpp`, and `ModelBlob.cpp` evidence the
//! stock model path. Binary parsing remains in `solarity-asset`.

mod character_component;
mod character_model_base;
mod component_utils;
pub(crate) mod m2_animation;
mod m2_scene;
mod world_model_scene;

pub use character_component::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterGeosetContext, CharacterGeosetPlan, CharacterGeosetPlanError, CharacterItemAttachment,
    CharacterItemVisualEffect, CharacterItemVisualPlan, CharacterSelectionQuiver,
    CharacterTabardMode, CharacterTextureComposeError, CharacterTexturePlan,
    CharacterTexturePlanError, CharacterWeaponState, CreatureGeosetPlan,
};
pub use m2_animation::{
    M2AnimationClock, M2BonePose, M2BonePoseError, M2CameraFrameError, M2EventTimeWindow,
    M2MaterialPose, M2MaterialPoseError, sample_m2_camera_frame, triggered_m2_event_indices,
};
pub use m2_scene::{
    M2DrawCall, M2DrawPushConstants, M2LocalLightState, M2MaterialUniform, M2MeshPlan,
    M2MeshPlanError, M2RenderVertex, M2SceneUniform, M2ShadowMatrix, M2ShadowState,
    M2TextureBinding, M2TransparentSortKey, compare_m2_transparent, m2_model_distance_key,
    m2_section_distance_key,
};
pub use world_model_scene::{
    PlacedWorldModelDrawPlan, WorldModelDrawCall, WorldModelGroupRange, WorldModelMaterialUniform,
    WorldModelMeshPlan, WorldModelMeshPlanError, WorldModelPlacementError, WorldModelRenderVertex,
    WorldModelSceneUniform,
};
