//! Frame-local procedural images updated only after their slot's fence retires.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::LiquidDepthTexture;
use crate::device::VulkanError;

/// One fixed-size depth image, with explicit cleanup on partial creation failure.
pub(super) struct DepthImage {
    image: vk::Image,
    allocation: Option<vk_mem::Allocation>,
    view: vk::ImageView,
}

impl DepthImage {
    pub(super) const fn empty() -> Self {
        Self {
            image: vk::Image::null(),
            allocation: None,
            view: vk::ImageView::null(),
        }
    }

    /// Allocates an uninitialized image; the next frame upload supplies every texel.
    pub(super) fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
    ) -> Result<(), VulkanError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width: LiquidDepthTexture::WIDTH,
                height: LiquidDepthTexture::HEIGHT,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            ..Default::default()
        };
        // SAFETY: The live allocator binds the returned optimal image allocation.
        let (image, memory) = unsafe { allocator.create_image(&info, &allocation) }
            .map_err(|source| VulkanError::operation("create liquid depth image", source))?;
        self.image = image;
        self.allocation = Some(memory);
        let view = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(color_range());
        // SAFETY: The view covers the sole color mip and layer of this image.
        self.view = unsafe { device.create_image_view(&view, None) }
            .map_err(|source| VulkanError::operation("create liquid depth image view", source))?;
        Ok(())
    }

    pub(super) const fn view(&self) -> vk::ImageView {
        self.view
    }

    /// Records a full replacement before the frame's first liquid sample.
    pub(super) fn upload(
        &self,
        device: &Device,
        command: vk::CommandBuffer,
        buffer: vk::Buffer,
        offset: u64,
    ) {
        let barrier = vk::ImageMemoryBarrier2::default()
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(self.image)
            .subresource_range(color_range());
        // SAFETY: The slot fence retired previous samples. Discarding old contents
        // is valid because this transfer replaces all texels before rendering.
        unsafe {
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
            )
        };
        let region = vk::BufferImageCopy::default()
            .buffer_offset(offset)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: LiquidDepthTexture::WIDTH,
                height: LiquidDepthTexture::HEIGHT,
                depth: 1,
            });
        // SAFETY: Frame allocation checks prove the staging range contains this image.
        unsafe {
            device.cmd_copy_buffer_to_image(
                command,
                buffer,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            )
        };
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(self.image)
            .subresource_range(color_range());
        // SAFETY: Synchronization2 orders the complete upload before liquid sampling.
        unsafe {
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
            )
        };
    }

    /// Releases the view before VMA image storage after the owning frame has retired.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Frame retirement covers all image use; each handle is uniquely owned.
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
                self.view = vk::ImageView::null();
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_image(self.image, &mut allocation);
                self.image = vk::Image::null();
            }
        }
    }
}

/// Describes the one complete color mip shared by all procedural depth images.
fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}
