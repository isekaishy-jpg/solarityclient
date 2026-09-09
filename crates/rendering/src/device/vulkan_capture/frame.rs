//! Explicit GPU readback of the completed client framebuffer for diagnostics.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use super::{VulkanError, scale};

/// One completed framebuffer in top-to-bottom, tightly packed RGBA8 order.
///
/// Pixels include postprocessing and UI, before desktop composition or display
/// gamma. Capturing does not apply another color conversion or resample them.
#[derive(Debug)]
pub struct CapturedFrame {
    extent: (u32, u32),
    rgba8: Vec<u8>,
}

impl CapturedFrame {
    /// Returns the framebuffer width and height in pixels.
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Returns exact stored channels, with four bytes per pixel and no row padding.
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }
}

/// A single requested copy, retained until its GPU work has retired.
pub(in crate::device) struct FrameReadback {
    pub(super) extent: (u32, u32),
    pub(super) byte_count: usize,
    buffer: vk::Buffer,
    pub(super) allocation: vk_mem::Allocation,
    pub(super) scale: Option<scale::CaptureScale>,
    /// Set only after a complete successful presentation; prevents later writes.
    pub(in crate::device) captured: bool,
}

impl FrameReadback {
    /// Allocates host-readable storage only for an explicit capture request.
    pub(in crate::device) fn create(
        allocator: &vk_mem::Allocator,
        extent: (u32, u32),
    ) -> Result<Self, VulkanError> {
        let byte_count = u64::from(extent.0)
            .checked_mul(u64::from(extent.1))
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes > 0 && *bytes <= isize::MAX as u64)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(VulkanError::FrameSize)?;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(byte_count as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM
                | vk_mem::AllocationCreateFlags::MAPPED,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds the requested host-visible allocation to this buffer.
        let (buffer, allocation) =
            unsafe { allocator.create_buffer(&buffer_info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create frame readback", source))?;
        Ok(Self {
            extent,
            byte_count,
            buffer,
            allocation,
            scale: None,
            captured: false,
        })
    }

    /// Replaces the final presentation transition with a copy and host barrier.
    ///
    /// The acquired BGRA8 image must match the source extent, with all rendering
    /// scopes ended. An optional scale image matches the bounded output extent.
    /// The caller retains both allocations until GPU completion is proven.
    pub(in crate::device) fn record(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
    ) {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let to_copy = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range)];
        // A failed presentation can leave the request pending after its copy
        // was submitted. Order any subsequent retry's write to the same buffer.
        let to_write = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer)
            .size(self.byte_count as u64)];
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: self.extent.0,
                height: self.extent.1,
                depth: 1,
            });
        let to_present = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range)];
        let to_host = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer)
            .size(self.byte_count as u64)];
        // SAFETY: The image is still acquired and transfer-readable after the
        // first barrier. Buffer and image footprints match exactly. Presentation
        // waits for all commands, including this copy and the final transition.
        unsafe {
            device.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(&to_copy)
                    .buffer_memory_barriers(&to_write),
            );
            let copy_image = self
                .scale
                .as_ref()
                .map_or(image, |scale| scale.record(device, command_buffer, image));
            device.cmd_copy_image_to_buffer(
                command_buffer,
                copy_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[region],
            );
            device.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(&to_present)
                    .buffer_memory_barriers(&to_host),
            );
        }
    }

    /// Copies completed mapped bytes after the renderer has waited for GPU idle.
    pub(in crate::device) fn read(
        &self,
        allocator: &vk_mem::Allocator,
    ) -> Result<CapturedFrame, VulkanError> {
        allocator
            .invalidate_allocation(&self.allocation, 0, self.byte_count as u64)
            .map_err(|source| VulkanError::operation("invalidate frame readback", source))?;
        let source = allocator
            .get_allocation_info(&self.allocation)
            .mapped_data
            .cast::<u8>();
        if source.is_null() {
            return Err(VulkanError::operation(
                "map frame readback",
                "allocation is not mapped",
            ));
        }
        // SAFETY: GPU idle and invalidation make the exact, checked allocation
        // footprint host-readable. The owned copy outlives its VMA allocation.
        let mut rgba8 = unsafe { std::slice::from_raw_parts(source, self.byte_count) }.to_vec();
        // The renderer requires B8G8R8A8_UNORM at adapter selection. Preserve
        // every stored channel value while exposing the public RGBA byte order.
        for pixel in rgba8.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Ok(CapturedFrame {
            extent: self.extent,
            rgba8,
        })
    }

    /// Releases the allocation after GPU idle, or before any submission.
    pub(in crate::device) fn destroy(mut self, allocator: &vk_mem::Allocator) {
        if let Some(scale) = self.scale.take() {
            scale.destroy(allocator);
        }
        // SAFETY: The owner has retired all commands referencing this buffer.
        unsafe { allocator.destroy_buffer(self.buffer, &mut self.allocation) };
    }
}
