//! Borrowed renderer resources pinned through scoped CPU recording and slot submission.

use ash::{Device, vk};

use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_celestial::CelestialFrameResources;
use crate::device::vulkan_cloud::CloudFrameResources;
use crate::device::vulkan_glow::{VulkanGlowRenderer, WorldFrameScreenEffect};
use crate::device::vulkan_liquid::{
    LiquidFrameResources, LiquidMeshRegistry, LiquidPipelines, LiquidPreparedDraw,
};
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_particle_draw::M2ParticlePreparedDraw;
use crate::device::vulkan_m2_particle_pipeline::M2ParticlePipelineRegistry;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_ribbon_draw::M2RibbonPreparedDraw;
use crate::device::vulkan_m2_ribbon_pipeline::M2RibbonPipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::vulkan_pct_pipeline::PctPipeline;
use crate::device::vulkan_ripple::{RippleFrameResources, WaterRippleFrame};
use crate::device::vulkan_sky::SkyFrameResources;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;
use crate::device::vulkan_ui_frame::UiOverlayRecordContext;
use crate::device::vulkan_underwater::{UnderwaterFrameResources, UnderwaterParticleFrame};
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::device::vulkan_world_model_mesh::WorldModelMeshRegistry;
use crate::device::vulkan_world_model_pipeline::WorldModelPipelineRegistry;
use crate::device::vulkan_world_model_texture_set::WorldModelTextureSetRegistry;

#[derive(Clone, Copy)]
pub(in crate::device::vulkan_world_frame) struct RecordContext<'a> {
    pub(in crate::device::vulkan_world_frame) color_format: vk::Format,
    pub(in crate::device::vulkan_world_frame) depth_format: vk::Format,
    /// Present only for sampled frames; the slot fence protects query reuse.
    pub(in crate::device::vulkan_world_frame) gpu_queries: Option<vk::QueryPool>,
    pub(in crate::device::vulkan_world_frame) scene: super::super::WorldFrameScene<'a>,
    pub(in crate::device::vulkan_world_frame) submission_fog: super::super::fog::SubmissionFog,
    pub(in crate::device::vulkan_world_frame) shadow_pipeline:
        &'a crate::device::vulkan_shadow::ShadowPipelines,
    pub(in crate::device::vulkan_world_frame) shadow_resources:
        &'a crate::device::vulkan_shadow::ShadowFrameResources,
    pub(in crate::device::vulkan_world_frame) shadow_frame:
        Option<crate::WorldPrimaryShadowFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) environment_frame:
        Option<crate::WorldEnvironmentShadowFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) environment_images:
        &'a crate::device::vulkan_shadow::EnvironmentShadowImages,
    pub(in crate::device::vulkan_world_frame) device: &'a Device,
    pub(in crate::device::vulkan_world_frame) capture: Option<&'a FrameReadback>,
    pub(in crate::device::vulkan_world_frame) command_buffer: vk::CommandBuffer,
    pub(in crate::device::vulkan_world_frame) post_command_buffer: vk::CommandBuffer,
    pub(in crate::device::vulkan_world_frame) image: vk::Image,
    pub(in crate::device::vulkan_world_frame) image_view: vk::ImageView,
    pub(in crate::device::vulkan_world_frame) depth_image: vk::Image,
    pub(in crate::device::vulkan_world_frame) depth_view: vk::ImageView,
    pub(in crate::device::vulkan_world_frame) extent: (u32, u32),
    pub(in crate::device::vulkan_world_frame) screen_window: crate::WorldScreenWindow,
    pub(in crate::device::vulkan_world_frame) sky_window: Option<crate::WorldSkyWindow>,
    pub(in crate::device::vulkan_world_frame) background_color: glam::Vec4,
    pub(in crate::device::vulkan_world_frame) frame_sets: [vk::DescriptorSet; 13],
    pub(in crate::device::vulkan_world_frame) world_model_material_stride: vk::DeviceSize,
    pub(in crate::device::vulkan_world_frame) m2_scene_stride: vk::DeviceSize,
    pub(in crate::device::vulkan_world_frame) terrain_pipelines: &'a TerrainPipelineRegistry,
    pub(in crate::device::vulkan_world_frame) terrain_meshes: &'a TerrainMeshRegistry,
    pub(in crate::device::vulkan_world_frame) terrain_texture_sets: &'a TerrainTextureSetRegistry,
    pub(in crate::device::vulkan_world_frame) liquid_pipelines: &'a LiquidPipelines,
    pub(in crate::device::vulkan_world_frame) liquid_meshes: &'a LiquidMeshRegistry,
    pub(in crate::device::vulkan_world_frame) liquid_resources: &'a LiquidFrameResources,
    pub(in crate::device::vulkan_world_frame) ripple_pipeline: &'a PctPipeline,
    pub(in crate::device::vulkan_world_frame) ripple_resources: &'a RippleFrameResources,
    pub(in crate::device::vulkan_world_frame) ripple_frame: Option<WaterRippleFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) underwater_pipeline: &'a PctPipeline,
    pub(in crate::device::vulkan_world_frame) underwater_resources: &'a UnderwaterFrameResources,
    pub(in crate::device::vulkan_world_frame) underwater_frame: Option<UnderwaterParticleFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) celestial_pipeline: &'a PctPipeline,
    pub(in crate::device::vulkan_world_frame) celestial_resources: &'a [CelestialFrameResources; 3],
    pub(in crate::device::vulkan_world_frame) celestial_frame:
        Option<crate::WorldCelestialFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) glare: &'a super::super::glare::GlareRenderer,
    pub(in crate::device::vulkan_world_frame) glare_slot: &'a super::super::glare::GlareSlot,
    pub(in crate::device::vulkan_world_frame) cloud_pipeline: &'a PctPipeline,
    pub(in crate::device::vulkan_world_frame) cloud_resources: &'a CloudFrameResources,
    pub(in crate::device::vulkan_world_frame) cloud_frame: Option<crate::WorldCloudFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) sky_pipeline: &'a PctPipeline,
    pub(in crate::device::vulkan_world_frame) low_detail_pipelines:
        &'a crate::device::vulkan_low_detail::LowDetailPipelines,
    pub(in crate::device::vulkan_world_frame) low_detail_map:
        Option<&'a crate::device::vulkan_low_detail::LowDetailGpuMap>,
    pub(in crate::device::vulkan_world_frame) low_detail_frame:
        Option<crate::WorldLowDetailFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) detail_pipeline:
        &'a crate::device::vulkan_detail::DetailPipeline,
    pub(in crate::device::vulkan_world_frame) ground_detail_registry:
        &'a crate::device::vulkan_detail::DetailRegistry,
    pub(in crate::device::vulkan_world_frame) ground_detail_frame:
        Option<crate::GroundDetailFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) depth_maximum: f32,
    pub(in crate::device::vulkan_world_frame) sky_resources: &'a SkyFrameResources,
    pub(in crate::device::vulkan_world_frame) sky_frame: Option<crate::WorldSkyFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) liquid_draws: &'a [LiquidPreparedDraw],
    pub(in crate::device::vulkan_world_frame) liquid_scene_order: u32,
    pub(in crate::device::vulkan_world_frame) world_model_pipelines: &'a WorldModelPipelineRegistry,
    pub(in crate::device::vulkan_world_frame) world_model_meshes: &'a WorldModelMeshRegistry,
    pub(in crate::device::vulkan_world_frame) world_model_texture_sets:
        &'a WorldModelTextureSetRegistry,
    pub(in crate::device::vulkan_world_frame) m2_pipelines: &'a M2PipelineRegistry,
    pub(in crate::device::vulkan_world_frame) m2_meshes: &'a M2MeshRegistry,
    pub(in crate::device::vulkan_world_frame) m2_texture_sets: &'a M2TextureSetRegistry,
    pub(in crate::device::vulkan_world_frame) m2_particle_pipelines: &'a M2ParticlePipelineRegistry,
    pub(in crate::device::vulkan_world_frame) m2_ribbon_pipelines: &'a M2RibbonPipelineRegistry,
    pub(in crate::device::vulkan_world_frame) terrain_draws: &'a [TerrainPreparedDraw],
    pub(in crate::device::vulkan_world_frame) world_model_draws: &'a [WorldModelPreparedDraw],
    pub(in crate::device::vulkan_world_frame) m2_draws: &'a [M2PreparedDraw],
    pub(in crate::device::vulkan_world_frame) sky_models:
        Option<super::super::WorldSkyModelFrame<'a>>,
    pub(in crate::device::vulkan_world_frame) particle_draws: &'a [M2ParticlePreparedDraw],
    pub(in crate::device::vulkan_world_frame) ribbon_draws: &'a [M2RibbonPreparedDraw],
    pub(in crate::device::vulkan_world_frame) particle_vertex_buffer: (vk::Buffer, vk::DeviceSize),
    pub(in crate::device::vulkan_world_frame) particle_index_buffer: (vk::Buffer, vk::DeviceSize),
    pub(in crate::device::vulkan_world_frame) ribbon_vertex_buffer: (vk::Buffer, vk::DeviceSize),
    pub(in crate::device::vulkan_world_frame) ui: Option<UiOverlayRecordContext<'a>>,
    pub(in crate::device::vulkan_world_frame) glow:
        Option<(&'a VulkanGlowRenderer, WorldFrameScreenEffect)>,
    pub(in crate::device::vulkan_world_frame) image_index: u32,
}
