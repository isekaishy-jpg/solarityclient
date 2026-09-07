//! Rendering state, resource, and presentation boundaries.

mod camera;
mod device;
mod effect;
mod geometry;
mod lighting;
mod liquid;
mod math;
mod minimap;
mod model;
mod particle;
mod scene;
mod shader;
mod terrain;
mod texture;
mod ui;
mod weather;
mod world_text;

pub use camera::{
    WORLD_DEPTH_MAXIMUM, WORLD_DEPTH_MINIMUM, WORLD_NEAR_CLIP,
    WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS, WorldCamera, WorldCameraError, WorldCameraFrame,
    WorldCameraProjection, WorldCameraSubject, WorldFrustum, WorldScreenWindow,
};
pub use device::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureSourceKind,
    BlpTextureStorage, BlpTextureUploadError, BlpTextureUploadRequest, CapturedFrame,
    CharacterAtlasTextureHandle, CharacterAtlasTextureResourceInfo, CinematicFrameIdentity,
    M2FrameReport, M2MeshHandle, M2MeshResourceInfo, M2ModelOrientation, M2ParticlePipelineHandle,
    M2ParticlePipelineInfo, M2ParticlePreparedDraw, M2PipelineHandle, M2PipelineInfo,
    M2PreparedDraw, M2RibbonPipelineHandle, M2RibbonPipelineInfo, M2RibbonPreparedDraw,
    M2SampledTexture, M2SamplerHandle, M2SamplerInfo, M2SceneLightBank, M2TextureAddressMode,
    M2TextureImageHandle, M2TextureSet, M2TextureSetHandle, M2TextureSetInfo, TerrainFrameReport,
    TerrainMaterialHandle, TerrainMaterialResourceInfo, TerrainMeshHandle, TerrainMeshResourceInfo,
    TerrainPipelineHandle, TerrainPipelineInfo, TerrainPreparedDraw, TerrainTextureSet,
    TerrainTextureSetHandle, TerrainTextureSetInfo, UiFrameReport, UiGlyphTextureHandle,
    UiGlyphTextureResourceInfo, UiMeshHandle, UiMeshResourceInfo, UiPipelineHandle, UiPipelineInfo,
    UiPortraitTextureHandle, UiPreparedDraw, UiSampledTexture, UiSamplerHandle, UiSamplerInfo,
    UiTextureImageHandle, UiTextureSetHandle, UiTextureSetInfo, VulkanBootstrap, VulkanError,
    VulkanPresentMode, VulkanRenderer, VulkanReport, WorldFrameGlow, WorldFrameReport,
    WorldFrameScene, WorldModelBaseMip, WorldModelMeshHandle, WorldModelMeshResourceInfo,
    WorldModelPipelineHandle, WorldModelPipelineInfo, WorldModelPreparedDraw,
    WorldModelSampledTexture, WorldModelSamplerHandle, WorldModelSamplerInfo,
    WorldModelTextureAddressMode, WorldModelTextureFiltering, WorldModelTextureSet,
    WorldModelTextureSetHandle, WorldModelTextureSetInfo,
};
pub use lighting::{
    M2DirectionalLight, M2LightOverride, M2PointLight, M2Sunlight, glue_character_sunlight,
    glue_ghost_sunlight, merge_wotlk_directional_lights,
};
pub use liquid::{
    LiquidDepthCoordinates, LiquidDepthTexture, LiquidDepthTextureKind, LiquidTextureTimeline,
};
pub use minimap::{MinimapView, MinimapViewError};
pub use model::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterComponentTextureLevel,
    CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan, CharacterGeosetPlanError,
    CharacterItemAttachment, CharacterItemVisualEffect, CharacterItemVisualPlan,
    CharacterSelectionQuiver, CharacterTabardMode, CharacterTextureComposeError,
    CharacterTexturePlan, CharacterTexturePlanError, CharacterWeaponState, CreatureGeosetPlan,
    M2AnimationClock, M2BonePose, M2BonePoseError, M2BonePoseOverrides, M2CameraEffectScale,
    M2CameraFrameError, M2DrawCall, M2DrawPushConstants, M2EffectOrder, M2ElementAlphaState,
    M2EventTimeWindow, M2FingerPoseHands, M2LocalLightState, M2MaterialPose, M2MaterialPoseError,
    M2MaterialUniform, M2MeshPlan, M2MeshPlanError, M2ModelSequenceBlend, M2ModelSequenceTimer,
    M2RenderVertex, M2SampledLights, M2SceneUniform, M2SequenceStartPhase, M2ShadowMatrix,
    M2ShadowState, M2TextureBinding, M2TransparentPass, M2TransparentSortKey, M2UiCameraViewport,
    PlacedWorldModelDrawPlan, WorldModelDrawCall, WorldModelGroupRange, WorldModelMaterialUniform,
    WorldModelMeshPlan, WorldModelMeshPlanError, WorldModelPlacementError, WorldModelRenderVertex,
    WorldModelSceneUniform, sample_m2_camera_frame, sample_m2_directional_lights, sample_m2_lights,
    sample_m2_lights_into, sample_m2_ui_camera_frame, triggered_m2_event_indices,
};
pub use model::{compare_m2_transparent, m2_model_distance_key, m2_section_distance_key};
pub use particle::{
    M2ParticleColorReplacement, M2ParticleLifetimePose, M2ParticleLifetimePoseError,
    M2ParticleMeshPlan, M2ParticleMeshPlanError, M2ParticlePose, M2ParticleRandom,
    M2ParticleRenderVertex, M2ParticleRotationPose, M2ParticleSimulation,
    M2ParticleSimulationError, M2ParticleSimulationReport, M2ParticleState, M2ParticleStateError,
    M2ParticleTwinkleError, M2ParticleTwinkleTable, M2RibbonControlPoint, M2RibbonMeshPlan,
    M2RibbonMeshPlanError, M2RibbonPose, M2RibbonRenderVertex, M2RibbonSection, M2RibbonTrail,
    M2RibbonTrailError,
};
pub use shader::{
    GlowShaderPass, GlowSpirvCompiler, GlowSpirvError, GlowSpirvProgram, M2BlendFactor, M2FogMode,
    M2LocalLightCount, M2MaterialState, M2ParticleSpirvCompiler, M2ParticleSpirvError,
    M2ParticleSpirvProgram, M2PixelShader, M2RibbonSpirvCompiler, M2RibbonSpirvError,
    M2RibbonSpirvProgram, M2ShaderPermutation, M2ShaderPlan, M2ShaderPlanError, M2ShadowFiltering,
    M2ShadowPermutation, M2SpirvCompiler, M2SpirvError, M2SpirvKey, M2SpirvProgram, M2VertexShader,
    TerrainLayerCount, TerrainLayerCountError, TerrainSpirvCompiler, TerrainSpirvError,
    TerrainSpirvProgram, UiShaderSource, UiSpirvCompiler, UiSpirvError, UiSpirvProgram,
    WorldModelBlendFactor, WorldModelBlendState, WorldModelFogMode, WorldModelLightingMode,
    WorldModelMaterialState, WorldModelSpirvCompiler, WorldModelSpirvError, WorldModelSpirvKey,
    WorldModelSpirvProgram, WorldModelSurfacePass, WorldModelSurfacePassPlan,
};
pub use terrain::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainChunkMeshPlan, TerrainRenderVertex, TerrainSceneUniform, TerrainTileMeshPlan,
    TerrainTileMeshPlanError,
};
pub use ui::{
    UiMeshPlan, UiMeshPlanError, UiRenderBatch, UiRenderBlend, UiRenderMask, UiRenderQuad,
    UiRenderSource, UiRenderState, UiRenderTransform, UiRenderVertex, UiTextureAddressMode,
    UiTextureResidency,
};
