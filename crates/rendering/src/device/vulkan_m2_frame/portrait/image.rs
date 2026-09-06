//! Owned color attachment with a sampled-image view and partial-failure cleanup.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::VulkanError;

pub(super) struct PortraitImage {
    pub(super) image: vk::Image,
    pub(super) view: vk::ImageView,
    allocation: vk_mem::Allocation,
}

impl PortraitImage {
    pub(super) fn create(
        device: &Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: (u32, u32),
    ) -> Result<Self, VulkanError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.0,
                height: extent.1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA binds and owns the returned image allocation.
        let (image, mut allocation) = unsafe { allocator.create_image(&info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create portrait image", source))?;
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range);
        // SAFETY: The image is live and this view covers its sole color subresource.
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(source) => {
                // SAFETY: No view or command references this newly allocated image.
                unsafe { allocator.destroy_image(image, &mut allocation) };
                return Err(VulkanError::operation("create portrait image view", source));
            }
        };
        Ok(Self {
            image,
            view,
            allocation,
        })
    }

    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Renderer teardown has retired all image and descriptor use.
        unsafe {
            device.destroy_image_view(self.view, None);
            allocator.destroy_image(self.image, &mut self.allocation);
        }
    }
}
