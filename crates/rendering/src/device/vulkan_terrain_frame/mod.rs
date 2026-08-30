//! Reusable swapchain frame slots for camera-selected terrain rendering.

#![allow(unsafe_code)]

mod command;
mod resource;
mod types;

use ash::{Device, vk};

use crate::TerrainSceneUniform;
use crate::device::VulkanError;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;

use command::{RecordContext, record_draws, submit_and_present};
use resource::{FrameCreateContext, TerrainFrameResources};

pub use types::TerrainFrameReport;

pub(in crate::device) struct TerrainFrameContext<'a> {
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
    pub(in crate::device) pipelines: &'a TerrainPipelineRegistry,
    pub(in crate::device) meshes: &'a TerrainMeshRegistry,
    pub(in crate::device) texture_sets: &'a TerrainTextureSetRegistry,
}

#[derive(Default)]
pub(in crate::device) struct TerrainFrameRenderer {
    resources: TerrainFrameResources,
}

impl TerrainFrameRenderer {
    pub(in crate::device) fn present(
        &mut self,
        context: TerrainFrameContext<'_>,
        scene_layout: vk::DescriptorSetLayout,
        scene: TerrainSceneUniform,
        draws: &[TerrainPreparedDraw],
    ) -> Result<TerrainFrameReport, VulkanError> {
        self.resources.ensure(FrameCreateContext {
            device: context.device,
            allocator: context.allocator,
            scene_layout,
            graphics_queue_family: context.graphics_queue_family,
            slot_count: context.swapchain_images.len(),
            extent: context.extent,
            depth_format: context.depth_format,
        })?;
        let slot_index = self.resources.next_slot_index()?;
        let (image_index, _suboptimal) = {
            let slot = self.resources.slot_mut(slot_index)?;
            slot.wait_and_reset(context.device)?;
            slot.write_scene(context.allocator, scene)?;
            // SAFETY: The acquire semaphore and swapchain remain live through presentation.
            unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| VulkanError::operation("acquire terrain frame image", source))?
        };
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::TerrainFrameCapacity)?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::TerrainFrameCapacity)?;
        let slot = self.resources.slot_mut(slot_index)?;
        record_draws(RecordContext {
            device: context.device,
            command_buffer: slot.command_buffer(),
            image,
            image_view,
            depth_image: slot.depth_image(),
            depth_view: slot.depth_view(),
            extent: context.extent,
            scene_set: slot.scene_set(),
            pipelines: context.pipelines,
            meshes: context.meshes,
            texture_sets: context.texture_sets,
            draws,
        })?;
        submit_and_present(&context, slot, present_semaphore, image_index)?;
        Ok(TerrainFrameReport::new(draws.len()))
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.resources.destroy(device, allocator);
    }
}
