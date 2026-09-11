//! Persistent environment images shared by ordered graphics-queue submissions.

#![allow(unsafe_code)]

use ash::{Device, vk};

use super::resource::Attachment;
use crate::{WorldShadowQuality, device::VulkanError};

/// Cached qualities retain two images per extent across every swapchain slot.
#[derive(Default)]
pub(in crate::device) struct EnvironmentShadowImages {
    quality: Option<WorldShadowQuality>,
    cache_id: u64,
    colors: [[Attachment; 2]; 3],
    depth: Attachment,
    initialized: bool,
}

impl EnvironmentShadowImages {
    /// Changes allocation only at a quality transition, after retiring all users.
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        depth_format: vk::Format,
        quality: WorldShadowQuality,
        cache_id: u64,
    ) -> Result<(), VulkanError> {
        if self.quality == Some(quality) && self.cache_id == cache_id {
            return Ok(());
        }
        // SAFETY: Quality transitions are rare. Retire every old descriptor user
        // before destroying shared images, including slots other than the next one.
        unsafe { device.device_wait_idle() }
            .map_err(|source| VulkanError::operation("retire environment shadow images", source))?;
        self.destroy(device, allocator);
        let size = quality
            .texture_size()
            .filter(|_| quality.shader_mode() > 1)
            .ok_or(VulkanError::M2ShadowResourcesUnavailable)?;
        let result = (|| {
            for pair in &mut self.colors {
                for color in pair
                    .iter_mut()
                    .take(if quality == WorldShadowQuality::Cascaded {
                        1
                    } else {
                        2
                    })
                {
                    color.create(
                        device,
                        allocator,
                        size,
                        vk::Format::R32_SFLOAT,
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::SAMPLED
                            | vk::ImageUsageFlags::TRANSFER_DST,
                        vk::ImageAspectFlags::COLOR,
                    )?;
                }
            }
            self.depth.create(
                device,
                allocator,
                size,
                depth_format,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
            )?;
            Ok(())
        })();
        if result.is_err() {
            self.destroy(device, allocator);
        }
        result?;
        self.quality = Some(quality);
        self.cache_id = cache_id;
        Ok(())
    }

    pub(in crate::device) fn receiver_views(&self, buffers: [usize; 3]) -> [vk::ImageView; 3] {
        std::array::from_fn(|index| self.colors[index][buffers[index]].view)
    }
    pub(in crate::device) fn color(&self, map: usize, buffer: usize) -> (vk::Image, vk::ImageView) {
        let color = &self.colors[map][buffer];
        (color.image, color.view)
    }
    pub(in crate::device) fn color_images(&self) -> impl Iterator<Item = vk::Image> + '_ {
        self.colors
            .iter()
            .flatten()
            .map(|color| color.image)
            .filter(|image| *image != vk::Image::null())
    }
    pub(in crate::device) const fn depth(&self) -> (vk::Image, vk::ImageView) {
        (self.depth.image, self.depth.view)
    }
    pub(in crate::device) const fn initialized(&self) -> bool {
        self.initialized
    }
    pub(in crate::device) const fn submitted(&mut self) {
        self.initialized = true;
    }

    /// The renderer retires queue users before invoking this teardown boundary.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        for color in self.colors.iter_mut().flatten() {
            color.destroy(device, allocator);
        }
        self.depth.destroy(device, allocator);
        self.quality = None;
        self.initialized = false;
    }
}
