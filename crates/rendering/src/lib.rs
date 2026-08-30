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
    WorldCameraSubject, WorldFrustum, WorldScreenWindow,
};
pub use device::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureUploadError, M2FrameReport,
    M2MeshHandle, M2MeshResourceInfo, M2PipelineHandle, M2PipelineInfo, M2PreparedDraw,
    M2SampledTexture, M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode, M2TextureSet,
    M2TextureSetHandle, M2TextureSetInfo, TerrainFrameReport, TerrainMaterialHandle,
    TerrainMaterialResourceInfo, TerrainMeshHandle, TerrainMeshResourceInfo, TerrainPipelineHandle,
    TerrainPipelineInfo, TerrainPreparedDraw, TerrainTextureSet, TerrainTextureSetHandle,
    TerrainTextureSetInfo, UiFrameReport, UiMeshHandle, UiMeshResourceInfo, UiPipelineHandle,
    UiPipelineInfo, UiPreparedDraw, UiSampledTexture, UiSamplerHandle, UiSamplerInfo,
    UiTextureSetHandle, UiTextureSetInfo, VulkanBootstrap, VulkanError, VulkanRenderer,
    VulkanReport, WorldFrameReport, WorldFrameScene, WorldModelBaseMip, WorldModelMeshHandle,
    WorldModelMeshResourceInfo, WorldModelPipelineHandle, WorldModelPipelineInfo,
    WorldModelPreparedDraw, WorldModelSampledTexture, WorldModelSamplerHandle,
    WorldModelSamplerInfo, WorldModelTextureAddressMode, WorldModelTextureFiltering,
    WorldModelTextureSet, WorldModelTextureSetHandle, WorldModelTextureSetInfo,
};
pub use model::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterGeosetContext, CharacterGeosetPlan, CharacterGeosetPlanError, CharacterItemAttachment,
    CharacterRangedHand, CharacterTabardMode, CharacterTextureComposeError, CharacterTexturePlan,
    CharacterTexturePlanError, CharacterWeaponPose, CharacterWeaponState, M2AnimationClock,
    M2BonePose, M2BonePoseError, M2DrawCall, M2DrawPushConstants, M2LocalLightState,
    M2MaterialUniform, M2MeshPlan, M2MeshPlanError, M2RenderVertex, M2SceneUniform, M2ShadowMatrix,
    M2ShadowState, M2TextureBinding, PlacedWorldModelDrawPlan, WorldModelDrawCall,
    WorldModelGroupRange, WorldModelMaterialUniform, WorldModelMeshPlan, WorldModelMeshPlanError,
    WorldModelPlacementError, WorldModelRenderVertex, WorldModelSceneUniform,
};
pub use shader::{
    M2BlendFactor, M2LocalLightCount, M2MaterialState, M2PixelShader, M2ShaderPermutation,
    M2ShaderPlan, M2ShaderPlanError, M2ShadowFiltering, M2ShadowPermutation, M2SpirvCompiler,
    M2SpirvError, M2SpirvKey, M2SpirvProgram, M2VertexShader, TerrainLayerCount,
    TerrainLayerCountError, TerrainSpirvCompiler, TerrainSpirvError, TerrainSpirvProgram,
    UiShaderSource, UiSpirvCompiler, UiSpirvError, UiSpirvProgram, WorldModelBlendFactor,
    WorldModelBlendState, WorldModelFogMode, WorldModelLightingMode, WorldModelMaterialState,
    WorldModelSpirvCompiler, WorldModelSpirvError, WorldModelSpirvKey, WorldModelSpirvProgram,
    WorldModelSurfacePass, WorldModelSurfacePassPlan,
};
pub use terrain::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainChunkMeshPlan, TerrainRenderVertex, TerrainSceneUniform, TerrainTileMeshPlan,
    TerrainTileMeshPlanError,
};
pub use ui::{
    UiMeshPlan, UiMeshPlanError, UiRenderBatch, UiRenderBlend, UiRenderQuad, UiRenderSource,
    UiRenderVertex, UiTextureAddressMode, UiTextureResidency,
};
