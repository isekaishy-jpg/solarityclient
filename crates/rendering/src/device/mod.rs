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
mod gfx_singleton_manager;
mod status;
mod vulkan_frame;
mod vulkan_instance;
mod vulkan_m2_draw;
mod vulkan_m2_frame;
mod vulkan_m2_pipeline;
mod vulkan_m2_texture_set;
mod vulkan_mesh;
mod vulkan_renderer;
mod vulkan_sampler;
mod vulkan_selection;
mod vulkan_texture;
mod vulkan_ui_draw;
mod vulkan_ui_frame;
mod vulkan_ui_mesh;
mod vulkan_ui_pipeline;
mod vulkan_ui_sampler;
mod vulkan_ui_texture_set;

pub use status::VulkanError;
pub use vulkan_instance::VulkanBootstrap;
pub use vulkan_m2_draw::M2PreparedDraw;
pub use vulkan_m2_frame::M2FrameReport;
pub use vulkan_m2_pipeline::{M2PipelineHandle, M2PipelineInfo};
pub use vulkan_m2_texture_set::{
    M2SampledTexture, M2TextureSet, M2TextureSetHandle, M2TextureSetInfo,
};
pub use vulkan_mesh::{M2MeshHandle, M2MeshResourceInfo};
pub use vulkan_renderer::{VulkanRenderer, VulkanReport};
pub use vulkan_sampler::{M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode};
pub use vulkan_texture::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureUploadError,
};
pub use vulkan_ui_draw::UiPreparedDraw;
pub use vulkan_ui_frame::UiFrameReport;
pub use vulkan_ui_mesh::{UiMeshHandle, UiMeshResourceInfo};
pub use vulkan_ui_pipeline::{UiPipelineHandle, UiPipelineInfo};
pub use vulkan_ui_sampler::{UiSamplerHandle, UiSamplerInfo};
pub use vulkan_ui_texture_set::{UiSampledTexture, UiTextureSetHandle, UiTextureSetInfo};
