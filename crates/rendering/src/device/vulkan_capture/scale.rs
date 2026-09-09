//! A retained transfer image bounds recording readback to 720p.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use super::VulkanError;

/// GPU-resident destination reused only after the preceding capture fence retires.
pub(super) struct CaptureScale {
    image: vk::Image,
    allocation: vk_mem::Allocation,
    source: (u32, u32),
    output: (u32, u32),
}

/// Preserve aspect ratio, use even codec dimensions, and never exceed 720p.
pub(super) fn recording_extent(source: (u32, u32)) -> (u32, u32) {
    let scale = (1280.0 / f64::from(source.0))
        .min(720.0 / f64::from(source.1))
        .min(1.0);
    (
        ((f64::from(source.0) * scale) as u32 & !1).max(2),
        ((f64::from(source.1) * scale) as u32 & !1).max(2),
    )
}

impl CaptureScale {
    /// Allocate one bounded transfer image for scaling and resize letterboxing.
    pub(super) fn create(
        allocator: &vk_mem::Allocator,
        source: (u32, u32),
        output: (u32, u32),
    ) -> Result<Self, VulkanError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
            .extent(vk::Extent3D {
                width: output.0,
                height: output.1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let memory = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            ..Default::default()
        };
        // SAFETY: VMA allocates and binds the complete transfer image.
        let (image, allocation) = unsafe { allocator.create_image(&info, &memory) }
            .map_err(|error| VulkanError::operation("create recording scale image", error))?;
        Ok(Self {
            image,
            allocation,
            source,
            output,
        })
    }

    /// Scale transfer-readable swapchain pixels into a cleared aspect-fit destination.
    pub(super) fn record(
        &self,
        device: &Device,
        command: vk::CommandBuffer,
        source: vk::Image,
    ) -> vk::Image {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let destination = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(range)];
        let aspect = (f64::from(self.output.0) / f64::from(self.source.0))
            .min(f64::from(self.output.1) / f64::from(self.source.1));
        let width = (f64::from(self.source.0) * aspect).round() as i32;
        let height = (f64::from(self.source.1) * aspect).round() as i32;
        let x = (self.output.0 as i32 - width) / 2;
        let y = (self.output.1 as i32 - height) / 2;
        let layer = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let region = [vk::ImageBlit::default()
            .src_subresource(layer)
            .dst_subresource(layer)
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: self.source.0 as i32,
                    y: self.source.1 as i32,
                    z: 1,
                },
            ])
            .dst_offsets([
                vk::Offset3D { x, y, z: 0 },
                vk::Offset3D {
                    x: x + width,
                    y: y + height,
                    z: 1,
                },
            ])];
        let readable = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(range)];
        // SAFETY: Source is transfer-readable; this retained destination's prior
        // copy has retired before reuse. Clear and blit writes are explicitly ordered.
        unsafe {
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&destination),
            );
            if width != self.output.0 as i32 || height != self.output.1 as i32 {
                device.cmd_clear_color_image(
                    command,
                    self.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                    &[range],
                );
                let ordered = [vk::MemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)];
                device.cmd_pipeline_barrier2(
                    command,
                    &vk::DependencyInfo::default().memory_barriers(&ordered),
                );
            }
            device.cmd_blit_image(
                command,
                source,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &region,
                vk::Filter::LINEAR,
            );
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&readable),
            );
        }
        self.image
    }

    /// Release a destination whose referencing GPU submissions have retired.
    pub(super) fn destroy(mut self, allocator: &vk_mem::Allocator) {
        // SAFETY: Owner has retired every submission that references this image.
        unsafe { allocator.destroy_image(self.image, &mut self.allocation) };
    }
}
