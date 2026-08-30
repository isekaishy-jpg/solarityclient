//! One reusable swapchain frame for terrain, WMO, and M2 world geometry.

#![allow(unsafe_code)]

mod command;
mod resource;
mod types;

use ash::{Device, vk};
use glam::Mat4;

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::device::vulkan_world_model_mesh::WorldModelMeshRegistry;
use crate::device::vulkan_world_model_pipeline::WorldModelPipelineRegistry;
use crate::device::vulkan_world_model_texture_set::WorldModelTextureSetRegistry;

use command::{RecordContext, record, submit_and_present};
use resource::{FrameCreateContext, WorldFrameResources};

pub use types::{WorldFrameReport, WorldFrameScene};

pub(in crate::device) struct WorldFrameContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) swapchain_loader: &'a ash::khr::swapchain::Device,
    pub(in crate::device) swapchain: vk::SwapchainKHR,
    pub(in crate::device) swapchain_images: &'a [vk::Image],
    pub(in crate::device) image_views: &'a [vk::ImageView],
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) present_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
    pub(in crate::device) extent: (u32, u32),
    pub(in crate::device) depth_format: vk::Format,
    pub(in crate::device) uniform_alignment: vk::DeviceSize,
    pub(in crate::device) storage_alignment: vk::DeviceSize,
    pub(in crate::device) terrain_pipelines: &'a TerrainPipelineRegistry,
    pub(in crate::device) terrain_meshes: &'a TerrainMeshRegistry,
    pub(in crate::device) terrain_texture_sets: &'a TerrainTextureSetRegistry,
    pub(in crate::device) world_model_pipelines: &'a WorldModelPipelineRegistry,
    pub(in crate::device) world_model_meshes: &'a WorldModelMeshRegistry,
    pub(in crate::device) world_model_texture_sets: &'a WorldModelTextureSetRegistry,
    pub(in crate::device) m2_pipelines: &'a M2PipelineRegistry,
    pub(in crate::device) m2_meshes: &'a M2MeshRegistry,
    pub(in crate::device) m2_texture_sets: &'a M2TextureSetRegistry,
}

#[derive(Default)]
pub(in crate::device) struct WorldFrameRenderer {
    resources: WorldFrameResources,
}

impl WorldFrameRenderer {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn present(
        &mut self,
        context: WorldFrameContext<'_>,
        descriptor_layouts: [vk::DescriptorSetLayout; 6],
        scene: WorldFrameScene,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
    ) -> Result<WorldFrameReport, VulkanError> {
        if terrain_draws.is_empty() && world_model_draws.is_empty() && m2_draws.is_empty() {
            return Err(VulkanError::EmptyWorldFrame);
        }
        if m2_draws.iter().any(|draw| {
            context
                .m2_pipelines
                .info(draw.pipeline())
                .is_some_and(|pipeline| pipeline.permutation().has_shadows())
        }) {
            return Err(VulkanError::M2ShadowResourcesUnavailable);
        }
        let required_bones = m2_draws
            .iter()
            .map(|draw| draw.required_bone_transforms())
            .max()
            .unwrap_or(0);
        if required_bones > bone_transforms.len() {
            return Err(VulkanError::M2FrameBoneTransforms {
                required: required_bones,
                available: bone_transforms.len(),
            });
        }
        self.resources.ensure(FrameCreateContext {
            device: context.device,
            allocator: context.allocator,
            descriptor_layouts,
            graphics_queue_family: context.graphics_queue_family,
            slot_count: context.swapchain_images.len(),
            world_model_draw_capacity: world_model_draws.len(),
            m2_draw_capacity: m2_draws.len(),
            bone_capacity: bone_transforms.len(),
            uniform_alignment: context.uniform_alignment,
            storage_alignment: context.storage_alignment,
            extent: context.extent,
            depth_format: context.depth_format,
        })?;
        let slot_index = self.resources.next_slot_index()?;
        let (image_index, _suboptimal) = {
            let slot = self.resources.slot_mut(slot_index)?;
            slot.wait_and_reset(context.device)?;
            slot.write(
                context.allocator,
                scene,
                bone_transforms,
                world_model_draws,
                m2_draws,
            )?;
            // SAFETY: Swapchain and acquire semaphore live through submission.
            unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| VulkanError::operation("acquire world frame image", source))?
        };
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let slot = self.resources.slot_mut(slot_index)?;
        record(RecordContext {
            device: context.device,
            command_buffer: slot.command_buffer(),
            image,
            image_view,
            depth_image: slot.depth_image(),
            depth_view: slot.depth_view(),
            extent: context.extent,
            frame_sets: slot.descriptor_sets(),
            world_model_material_stride: slot.world_model_material_stride(),
            m2_material_stride: slot.m2_material_stride(),
            terrain_pipelines: context.terrain_pipelines,
            terrain_meshes: context.terrain_meshes,
            terrain_texture_sets: context.terrain_texture_sets,
            world_model_pipelines: context.world_model_pipelines,
            world_model_meshes: context.world_model_meshes,
            world_model_texture_sets: context.world_model_texture_sets,
            m2_pipelines: context.m2_pipelines,
            m2_meshes: context.m2_meshes,
            m2_texture_sets: context.m2_texture_sets,
            terrain_draws,
            world_model_draws,
            m2_draws,
        })?;
        submit_and_present(&context, slot, present_semaphore, image_index)?;
        Ok(WorldFrameReport::new(
            terrain_draws.len(),
            world_model_draws.len(),
            m2_draws.len(),
            bone_transforms.len(),
        ))
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.resources.destroy(device, allocator);
    }
}
