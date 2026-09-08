//! Vulkan 1.3 device, queue, allocator, swapchain, and presentation ownership.
//!
//! Stock groups equivalent responsibilities under `CGxDevice.cpp` and its D3D
//! and OpenGL backends. Solarity has one explicit Vulkan backend through `ash`;
//! it does not add an unobserved runtime renderer fallback.

mod c_gx_d3d9_ex_device;
mod c_gx_d3d9_ex_texture;
mod c_gx_d3d_device;
mod c_gx_device;
mod c_gx_device_d3d;
mod c_gx_device_d3d9_ex;
mod c_gx_device_open_gl;
mod capacity;
mod gfx_singleton_manager;
mod m2_model_orientation;
mod status;
mod vulkan_capture;
mod vulkan_character_atlas;
mod vulkan_frame;
mod vulkan_glow;
mod vulkan_instance;
mod vulkan_liquid;
mod vulkan_m2_draw;
mod vulkan_m2_frame;
mod vulkan_m2_particle_draw;
mod vulkan_m2_particle_pipeline;
mod vulkan_m2_pipeline;
mod vulkan_m2_ribbon_draw;
mod vulkan_m2_ribbon_pipeline;
mod vulkan_m2_texture_set;
mod vulkan_mesh;
mod vulkan_pct_pipeline;
mod vulkan_renderer;
mod vulkan_ripple;
mod vulkan_sampler;
mod vulkan_selection;
mod vulkan_sky;
mod vulkan_terrain_draw;
mod vulkan_terrain_frame;
mod vulkan_terrain_material;
mod vulkan_terrain_mesh;
mod vulkan_terrain_pipeline;
mod vulkan_terrain_texture_set;
mod vulkan_texture;
mod vulkan_ui_draw;
mod vulkan_ui_frame;
mod vulkan_ui_glyph_texture;
mod vulkan_ui_mesh;
mod vulkan_ui_pipeline;
mod vulkan_ui_sampler;
mod vulkan_ui_texture_set;
mod vulkan_underwater;
mod vulkan_world_frame;
mod vulkan_world_model_draw;
mod vulkan_world_model_mesh;
mod vulkan_world_model_pipeline;
mod vulkan_world_model_sampler;
mod vulkan_world_model_texture_set;

pub use m2_model_orientation::M2ModelOrientation;
pub use status::VulkanError;
pub use vulkan_capture::CapturedFrame;
pub use vulkan_character_atlas::{CharacterAtlasTextureHandle, CharacterAtlasTextureResourceInfo};
pub use vulkan_frame::CinematicFrameIdentity;
pub use vulkan_glow::WorldFrameGlow;
pub use vulkan_instance::VulkanBootstrap;
pub use vulkan_liquid::{LiquidDrawMaterial, LiquidFrame, LiquidMeshHandle, LiquidPreparedDraw};
pub use vulkan_m2_draw::{M2PreparedDraw, M2SceneLightBank};
pub use vulkan_m2_frame::{M2FrameReport, UiPortraitTextureHandle};
pub use vulkan_m2_particle_draw::M2ParticlePreparedDraw;
pub use vulkan_m2_particle_pipeline::{M2ParticlePipelineHandle, M2ParticlePipelineInfo};
pub use vulkan_m2_pipeline::{M2PipelineHandle, M2PipelineInfo};
pub use vulkan_m2_ribbon_draw::M2RibbonPreparedDraw;
pub use vulkan_m2_ribbon_pipeline::{M2RibbonPipelineHandle, M2RibbonPipelineInfo};
pub use vulkan_m2_texture_set::{
    M2SampledTexture, M2TextureImageHandle, M2TextureSet, M2TextureSetHandle, M2TextureSetInfo,
};
pub use vulkan_mesh::{M2MeshHandle, M2MeshResourceInfo};
pub use vulkan_renderer::{VulkanPresentMode, VulkanRenderer, VulkanReport};
pub use vulkan_ripple::{WaterRippleFrame, WaterRippleFrameError, WaterRipplePass};
pub use vulkan_sampler::{M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode};
pub use vulkan_terrain_draw::TerrainPreparedDraw;
pub use vulkan_terrain_frame::TerrainFrameReport;
pub use vulkan_terrain_material::{TerrainMaterialHandle, TerrainMaterialResourceInfo};
pub use vulkan_terrain_mesh::{TerrainMeshHandle, TerrainMeshResourceInfo};
pub use vulkan_terrain_pipeline::{TerrainPipelineHandle, TerrainPipelineInfo};
pub use vulkan_terrain_texture_set::{
    TerrainTextureSet, TerrainTextureSetHandle, TerrainTextureSetInfo,
};
pub use vulkan_texture::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureSourceKind,
    BlpTextureStorage, BlpTextureUploadError, BlpTextureUploadRequest,
};
pub use vulkan_ui_draw::UiPreparedDraw;
pub use vulkan_ui_frame::UiFrameReport;
pub use vulkan_ui_glyph_texture::{UiGlyphTextureHandle, UiGlyphTextureResourceInfo};
pub use vulkan_ui_mesh::{UiMeshHandle, UiMeshResourceInfo};
pub use vulkan_ui_pipeline::{UiPipelineHandle, UiPipelineInfo};
pub use vulkan_ui_sampler::{UiSamplerHandle, UiSamplerInfo};
pub use vulkan_ui_texture_set::{
    UiSampledTexture, UiTextureImageHandle, UiTextureSetHandle, UiTextureSetInfo,
};
pub use vulkan_underwater::{
    UnderwaterParticleFog, UnderwaterParticleFrame, UnderwaterParticleFrameError,
};
pub use vulkan_world_frame::{WorldFrameReport, WorldFrameScene};
pub use vulkan_world_model_draw::WorldModelPreparedDraw;
pub use vulkan_world_model_mesh::{WorldModelMeshHandle, WorldModelMeshResourceInfo};
pub use vulkan_world_model_pipeline::{WorldModelPipelineHandle, WorldModelPipelineInfo};
pub use vulkan_world_model_sampler::{
    WorldModelBaseMip, WorldModelSamplerHandle, WorldModelSamplerInfo,
    WorldModelTextureAddressMode, WorldModelTextureFiltering,
};
pub use vulkan_world_model_texture_set::{
    WorldModelSampledTexture, WorldModelTextureSet, WorldModelTextureSetHandle,
    WorldModelTextureSetInfo,
};
