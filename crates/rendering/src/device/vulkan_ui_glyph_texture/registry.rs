//! Identity deduplication and Vulkan lifetime ownership for UI glyph atlases.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::device::vulkan_texture::{
    DeferredTextureTransfer, GpuSampledImage, TextureUploadContext, update_rgba8_regions,
    upload_rgba8_image_deferred,
};
use crate::device::{VulkanError, vulkan_ui_glyph_texture::UiGlyphTextureResourceInfo};

use super::UiGlyphTextureHandle;

/// One device image joined to its immutable CPU generation facts.
struct GpuUiGlyphTexture {
    image: GpuSampledImage,
    info: UiGlyphTextureResourceInfo,
}

/// Owns live glyph pages until their coverage bank and prepared frames depart.
pub(in crate::device) struct UiGlyphTextureRegistry {
    registry_id: u64,
    handles: HashMap<u64, UiGlyphTextureHandle>,
    resources: HashMap<u32, GpuUiGlyphTexture>,
    next_slot: u32,
    pending: Vec<DeferredTextureTransfer>,
}

impl Default for UiGlyphTextureRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: HashMap::new(),
            next_slot: 0,
            pending: Vec::new(),
        }
    }
}

impl UiGlyphTextureRegistry {
    /// On-demand content accounting, outside ordinary frame preparation.
    pub(in crate::device) fn usage(&self) -> (usize, usize) {
        (
            self.resources.len(),
            self.resources
                .values()
                .map(|resource| resource.info.upload_byte_count())
                .sum(),
        )
    }

    /// Returns the resident generation or uploads one tightly packed atlas.
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        identity: u64,
        extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<UiGlyphTextureHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&identity) {
            return Ok(*handle);
        }
        let slot = self.next_slot;
        self.next_slot = slot
            .checked_add(1)
            .ok_or(VulkanError::UiGlyphTextureCapacity)?;
        self.retire_transfers(context)?;
        let (image, transfer) = upload_rgba8_image_deferred(context, extent, rgba8)?;
        self.pending.push(transfer);
        let handle = UiGlyphTextureHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.insert(
            slot,
            GpuUiGlyphTexture {
                image,
                info: UiGlyphTextureResourceInfo::new(identity, extent, rgba8.len()),
            },
        );
        self.handles.insert(identity, handle);
        Ok(handle)
    }

    /// Appends coverage to an existing page without changing its image view.
    pub(in crate::device) fn update(
        &mut self,
        context: TextureUploadContext<'_>,
        handle: UiGlyphTextureHandle,
        extent: (u32, u32),
        bytes: &[u8],
        rectangles: &[[u32; 4]],
    ) -> Result<(), VulkanError> {
        self.retire_transfers(context)?;
        if handle.registry_id != self.registry_id {
            return Err(VulkanError::UnknownUiGlyphTextureHandle);
        }
        let resource = self
            .resources
            .get(&handle.slot)
            .ok_or(VulkanError::UnknownUiGlyphTextureHandle)?;
        if resource.info.extent() != extent {
            return Err(VulkanError::UiDrawTextureMismatch);
        }
        if !rectangles.is_empty() {
            self.pending.push(update_rgba8_regions(
                context,
                &resource.image,
                extent,
                bytes,
                rectangles,
            )?);
        }
        Ok(())
    }

    /// Completed staging transfers release without blocking unfinished work.
    pub(in crate::device) fn retire_transfers(
        &mut self,
        context: TextureUploadContext<'_>,
    ) -> Result<(), VulkanError> {
        for index in (0..self.pending.len()).rev() {
            if self.pending[index].is_complete(context.device)? {
                self.pending
                    .swap_remove(index)
                    .destroy(context.device, context.allocator);
            }
        }
        Ok(())
    }

    /// Invalidates the CPU handle and transfers image ownership to a fence batch.
    pub(in crate::device) fn take(
        &mut self,
        handle: UiGlyphTextureHandle,
    ) -> Option<GpuSampledImage> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        let resource = self.resources.remove(&handle.slot)?;
        self.handles.remove(&resource.info.identity());
        Some(resource.image)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan handles.
    pub(in crate::device) fn info(
        &self,
        handle: UiGlyphTextureHandle,
    ) -> Option<UiGlyphTextureResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources.get(&handle.slot).map(|item| item.info)
    }

    /// Resolves a renderer-local identity to its live sampled image view.
    pub(in crate::device) fn view(&self, handle: UiGlyphTextureHandle) -> Option<vk::ImageView> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|item| item.image.view())
    }

    /// Releases views and images in reverse admission order.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        for mut transfer in self.pending.drain(..) {
            transfer.destroy(device, allocator);
        }
        self.handles.clear();
        for mut resource in self.resources.drain().map(|(_, resource)| resource) {
            resource.image.destroy(device, allocator);
        }
    }
}
