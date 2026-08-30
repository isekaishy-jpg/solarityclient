//! Reusable swapchain frame slots for indexed M2 rendering.

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
use crate::model::M2SceneUniform;

use command::{RecordContext, record_draws, submit_and_present};
use resource::{FrameCreateContext, M2FrameResources};

pub use types::M2FrameReport;

/// Borrowed renderer graph required to present one M2 scene snapshot.
pub(in crate::device) struct M2FrameContext<'a> {
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
    pub(in crate::device) pipelines: &'a M2PipelineRegistry,
    pub(in crate::device) meshes: &'a M2MeshRegistry,
    pub(in crate::device) texture_sets: &'a M2TextureSetRegistry,
}

/// Owns and advances reusable per-swapchain M2 frame resources.
#[derive(Default)]
pub(in crate::device) struct M2FrameRenderer {
    resources: M2FrameResources,
}

impl M2FrameRenderer {
    /// Uploads one scene snapshot, records indexed draws, and queues presentation.
    pub(in crate::device) fn present(
        &mut self,
        context: M2FrameContext<'_>,
        frame_layouts: [vk::DescriptorSetLayout; 3],
        scene: M2SceneUniform,
        bone_transforms: &[Mat4],
        draws: &[M2PreparedDraw],
    ) -> Result<M2FrameReport, VulkanError> {
        if draws.is_empty() {
            return Err(VulkanError::EmptyM2Frame);
        }
        // Shader translation can be validated independently, but recording a
        // shadowed draw without set four would violate the Vulkan pipeline ABI.
        if draws.iter().any(|draw| {
            context
                .pipelines
                .info(draw.pipeline())
                .is_some_and(|pipeline| pipeline.permutation().has_shadows())
        }) {
            return Err(VulkanError::M2ShadowResourcesUnavailable);
        }
        let required_bones = draws
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
            descriptor_layouts: frame_layouts,
            graphics_queue_family: context.graphics_queue_family,
            slot_count: context.swapchain_images.len(),
            draw_capacity: draws.len(),
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
            slot.write(context.allocator, scene, bone_transforms, draws)?;
            // SAFETY: The swapchain and slot acquire semaphore remain live until
            // the returned image is submitted and presented below.
            unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| VulkanError::operation("acquire M2 frame image", source))?
        };
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("index M2 frame image", "index is out of range")
            })?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("index M2 frame view", "index is out of range")
            })?;
        let slot = self.resources.slot_mut(slot_index)?;
        record_draws(RecordContext {
            device: context.device,
            command_buffer: slot.command_buffer(),
            image,
            image_view,
            depth_image: slot.depth_image(),
            depth_view: slot.depth_view(),
            extent: context.extent,
            frame_sets: slot.descriptor_sets(),
            material_stride: slot.material_stride(),
            pipelines: context.pipelines,
            meshes: context.meshes,
            texture_sets: context.texture_sets,
            draws,
        })?;
        submit_and_present(&context, slot, present_semaphore, image_index)?;
        Ok(M2FrameReport::new(draws.len(), bone_transforms.len()))
    }

    /// Releases all persistent frame children before their allocator/device.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.resources.destroy(device, allocator);
    }
}
