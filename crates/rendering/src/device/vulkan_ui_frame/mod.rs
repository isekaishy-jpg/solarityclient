//! Reusable swapchain frame slots for ordered stock UI rendering.

#![allow(unsafe_code)]

mod command;
mod resource;
mod types;

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;

use command::{RecordContext, record_draws, submit_and_present};
pub(in crate::device) use command::{UiOverlayRecordContext, record_loaded_overlay};
use resource::UiFrameResources;

pub use types::UiFrameReport;

/// Borrowed renderer graph required to present one immutable UI generation.
pub(in crate::device) struct UiFrameContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) capture: Option<&'a FrameReadback>,
    pub(in crate::device) swapchain_loader: &'a ash::khr::swapchain::Device,
    pub(in crate::device) swapchain: vk::SwapchainKHR,
    pub(in crate::device) swapchain_images: &'a [vk::Image],
    pub(in crate::device) image_views: &'a [vk::ImageView],
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) present_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
    pub(in crate::device) extent: (u32, u32),
    pub(in crate::device) pipelines: &'a UiPipelineRegistry,
    pub(in crate::device) meshes: &'a UiMeshRegistry,
    pub(in crate::device) texture_sets: &'a UiTextureSetRegistry,
}

/// Owns and advances reusable per-swapchain UI command resources.
#[derive(Default)]
pub(in crate::device) struct UiFrameRenderer {
    resources: UiFrameResources,
}

impl UiFrameRenderer {
    /// Records two already ordered UI draw layers without joining their storage.
    pub(in crate::device) fn present_composite(
        &mut self,
        context: UiFrameContext<'_>,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
        overlay: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        if draws.is_empty() && overlay.is_empty() {
            return Err(VulkanError::EmptyUiFrame);
        }
        self.present_inner(context, logical_extent, draws, overlay)
    }

    /// Clears and presents one swapchain image without requiring UI geometry.
    pub(in crate::device) fn present_clear(
        &mut self,
        context: UiFrameContext<'_>,
        logical_extent: [f32; 2],
    ) -> Result<UiFrameReport, VulkanError> {
        self.present_inner(context, logical_extent, &[], &[])
    }

    fn present_inner(
        &mut self,
        context: UiFrameContext<'_>,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
        overlay: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        if logical_extent
            .iter()
            .any(|extent| !extent.is_finite() || *extent <= 0.0)
        {
            return Err(VulkanError::UiFrameExtent);
        }
        self.resources.ensure(
            context.device,
            context.graphics_queue_family,
            context.swapchain_images.len(),
        )?;
        let slot_index = self.resources.next_slot_index()?;
        let (image_index, _suboptimal) = {
            let slot = self.resources.slot_mut(slot_index)?;
            slot.wait_and_reset(context.device)?;
            // SAFETY: The swapchain and acquire semaphore remain live until
            // this exact image is submitted and presented below.
            unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| swapchain_error("acquire UI frame image", source))?
        };
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::UiFrameCapacity)?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::UiFrameCapacity)?;
        let slot = self.resources.slot_mut(slot_index)?;
        record_draws(RecordContext {
            device: context.device,
            capture: context.capture,
            command_buffer: slot.command_buffer(),
            image,
            image_view,
            extent: context.extent,
            logical_extent,
            pipelines: context.pipelines,
            meshes: context.meshes,
            texture_sets: context.texture_sets,
            draws,
            overlay,
        })?;
        submit_and_present(&context, slot, present_semaphore, image_index)?;
        Ok(UiFrameReport::new(draws.len() + overlay.len()))
    }

    /// Releases all persistent command and synchronization children.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.resources.destroy(device);
    }
}
