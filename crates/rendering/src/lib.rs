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
mod weather;
mod world_text;

pub use device::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureUploadError, M2MeshHandle,
    M2MeshResourceInfo, M2PipelineHandle, M2PipelineInfo, M2PreparedDraw, M2SampledTexture,
    M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode, M2TextureSet, M2TextureSetHandle,
    M2TextureSetInfo, VulkanBootstrap, VulkanError, VulkanRenderer, VulkanReport,
};
pub use model::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasRegion, CharacterAtlasTexture, CharacterAttachmentPlan,
    CharacterAttachmentPlanError, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterGeosetContext, CharacterGeosetPlan, CharacterGeosetPlanError, CharacterItemAttachment,
    CharacterRangedHand, CharacterTabardMode, CharacterTextureComposeError, CharacterTexturePlan,
    CharacterTexturePlanError, CharacterWeaponPose, CharacterWeaponState, M2DrawCall,
    M2DrawPushConstants, M2LocalLightState, M2MaterialUniform, M2MeshPlan, M2MeshPlanError,
    M2RenderVertex, M2SceneUniform, M2TextureBinding,
};
pub use shader::{
    M2BlendFactor, M2LocalLightCount, M2MaterialState, M2PixelShader, M2ShaderPermutation,
    M2ShaderPlan, M2ShaderPlanError, M2ShadowFiltering, M2ShadowPermutation, M2SpirvCompiler,
    M2SpirvError, M2SpirvKey, M2SpirvProgram, M2VertexShader,
};
