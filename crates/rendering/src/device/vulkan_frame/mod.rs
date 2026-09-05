//! Retained decoded-frame upload and linearly filtered presentation.

#![allow(unsafe_code)]

mod command;
mod resource;
mod types;

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;

use command::{RecordContext, record_frame, submit_and_present};
use resource::FrameResources;
pub use types::CinematicFrameIdentity;

/// Retained UI state appended after a decoded movie frame.
#[derive(Clone, Copy)]
pub(super) struct FrameUiContext<'a> {
    pub(super) logical_extent: [f32; 2],
    pub(super) pipelines: &'a UiPipelineRegistry,
    pub(super) meshes: &'a UiMeshRegistry,
    pub(super) texture_sets: &'a UiTextureSetRegistry,
    pub(super) draws: &'a [UiPreparedDraw],
}

/// Borrowed live Vulkan objects needed for one presentation.
pub(super) struct FrameContext<'a> {
    pub(super) device: &'a Device,
    pub(super) allocator: &'a vk_mem::Allocator,
    pub(super) capture: Option<&'a FrameReadback>,
    pub(super) swapchain_loader: &'a ash::khr::swapchain::Device,
    pub(super) swapchain: vk::SwapchainKHR,
    pub(super) swapchain_images: &'a [vk::Image],
    pub(super) image_views: &'a [vk::ImageView],
    pub(super) graphics_queue: vk::Queue,
    pub(super) present_queue: vk::Queue,
    pub(super) graphics_queue_family: u32,
    pub(super) frame_extent: (u32, u32),
    pub(super) source_extent: (u32, u32),
    pub(super) rgba8: &'a [u8],
    pub(super) identity: Option<CinematicFrameIdentity>,
    pub(super) ui: Option<FrameUiContext<'a>>,
}

/// Owns the source image and reusable swapchain command slots.
#[derive(Default)]
pub(super) struct FrameRenderer {
    resources: FrameResources,
}

impl FrameRenderer {
    /// Uploads a changed authored frame and presents it without CPU resampling.
    pub(super) fn present(&mut self, context: FrameContext<'_>) -> Result<bool, VulkanError> {
        validate_pixels(context.source_extent, context.rgba8)?;
        let layout = FrameLayout::fit(context.frame_extent, context.source_extent)?;
        self.resources.ensure_slots(
            context.device,
            context.graphics_queue_family,
            context.swapchain_images.len(),
        )?;

        let upload_source = self
            .resources
            .requires_upload(context.source_extent, context.identity);
        if upload_source {
            // Ghidra `CSimpleMovieFrame.cpp` update at 0x0095EBF0 changes the
            // retained decoded surfaces only when the authored frame advances.
            self.resources.wait_all(context.device)?;
            self.resources.ensure_source(
                context.device,
                context.allocator,
                context.source_extent,
            )?;
            self.resources
                .write_source(context.allocator, context.rgba8)?;
        }

        let slot_index = self.resources.next_slot_index()?;
        let (image_index, _suboptimal) = {
            let slot = self.resources.slot_mut(slot_index)?;
            slot.wait_and_reset(context.device)?;
            // SAFETY: The swapchain and semaphore stay live through submission.
            unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| swapchain_error("acquire cinematic frame image", source))?
        };
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::FrameCapacity)?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::FrameCapacity)?;
        let source_image = self.resources.source_image()?;
        let source_buffer = self.resources.source_buffer()?;
        let source_initialized = self.resources.source_initialized()?;
        let slot = self.resources.slot_mut(slot_index)?;
        record_frame(RecordContext {
            device: context.device,
            capture: context.capture,
            command_buffer: slot.command_buffer(),
            source_image,
            source_buffer,
            source_extent: context.source_extent,
            source_initialized,
            upload_source,
            image,
            image_view,
            frame_extent: context.frame_extent,
            source_end: layout.source_end,
            destination: layout.destination,
            ui: context.ui,
        })?;
        submit_and_present(&context, slot, present_semaphore, image_index)?;
        if upload_source {
            self.resources.commit_upload(context.identity)?;
        }
        Ok(upload_source)
    }

    /// Releases all retained source and swapchain-shaped resources.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.resources.destroy(device, allocator);
    }
}

/// Exact integer aspect-fit result used as a Vulkan blit rectangle.
struct FrameLayout {
    source_end: vk::Offset3D,
    destination: [vk::Offset3D; 2],
}

impl FrameLayout {
    /// Fits the source inside the destination without changing its aspect ratio.
    fn fit(frame: (u32, u32), source: (u32, u32)) -> Result<Self, VulkanError> {
        if frame.0 == 0 || frame.1 == 0 || source.0 == 0 || source.1 == 0 {
            return Err(VulkanError::FrameSize);
        }
        let frame_aspect = u64::from(frame.0) * u64::from(source.1);
        let source_aspect = u64::from(source.0) * u64::from(frame.1);
        let (width, height) = if source_aspect > frame_aspect {
            let height = u64::from(source.1) * u64::from(frame.0) / u64::from(source.0);
            (u64::from(frame.0), height.max(1))
        } else {
            let width = u64::from(source.0) * u64::from(frame.1) / u64::from(source.1);
            (width.max(1), u64::from(frame.1))
        };
        let width = i32::try_from(width).map_err(|_source| VulkanError::FrameSize)?;
        let height = i32::try_from(height).map_err(|_source| VulkanError::FrameSize)?;
        let frame_width = i32::try_from(frame.0).map_err(|_source| VulkanError::FrameSize)?;
        let frame_height = i32::try_from(frame.1).map_err(|_source| VulkanError::FrameSize)?;
        let source_width = i32::try_from(source.0).map_err(|_source| VulkanError::FrameSize)?;
        let source_height = i32::try_from(source.1).map_err(|_source| VulkanError::FrameSize)?;
        let origin_x = (frame_width - width) / 2;
        let origin_y = (frame_height - height) / 2;
        Ok(Self {
            source_end: vk::Offset3D {
                x: source_width,
                y: source_height,
                z: 1,
            },
            destination: [
                vk::Offset3D {
                    x: origin_x,
                    y: origin_y,
                    z: 0,
                },
                vk::Offset3D {
                    x: origin_x + width,
                    y: origin_y + height,
                    z: 1,
                },
            ],
        })
    }
}

/// Validates one tightly packed RGBA8 source before any Vulkan mutation.
fn validate_pixels(extent: (u32, u32), rgba8: &[u8]) -> Result<(), VulkanError> {
    let expected = usize::try_from(extent.0)
        .ok()
        .and_then(|width| {
            usize::try_from(extent.1)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(VulkanError::FrameSize)?;
    if extent.0 == 0 || extent.1 == 0 || rgba8.len() != expected {
        return Err(VulkanError::FrameSize);
    }
    Ok(())
}

pub(super) fn swapchain_error(operation: &'static str, source: vk::Result) -> VulkanError {
    if source == vk::Result::ERROR_OUT_OF_DATE_KHR {
        VulkanError::SwapchainOutOfDate
    } else {
        VulkanError::operation(operation, source)
    }
}
