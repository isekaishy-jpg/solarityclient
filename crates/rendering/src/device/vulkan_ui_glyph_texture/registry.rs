//! Identity deduplication and Vulkan lifetime ownership for UI glyph atlases.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::device::vulkan_texture::{GpuSampledImage, TextureUploadContext, upload_rgba8_image};
use crate::device::{VulkanError, vulkan_ui_glyph_texture::UiGlyphTextureResourceInfo};

use super::UiGlyphTextureHandle;

/// One device image joined to its immutable CPU generation facts.
struct GpuUiGlyphTexture {
    image: GpuSampledImage,
    info: UiGlyphTextureResourceInfo,
}

/// Owns every unique UI coverage generation until renderer teardown.
pub(in crate::device) struct UiGlyphTextureRegistry {
    registry_id: u64,
    handles: HashMap<u64, UiGlyphTextureHandle>,
    resources: Vec<GpuUiGlyphTexture>,
}

impl Default for UiGlyphTextureRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl UiGlyphTextureRegistry {
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
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::UiGlyphTextureCapacity)?;
        let image = upload_rgba8_image(context, extent, rgba8)?;
        let handle = UiGlyphTextureHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuUiGlyphTexture {
            image,
            info: UiGlyphTextureResourceInfo::new(identity, extent, rgba8.len()),
        });
        self.handles.insert(identity, handle);
        Ok(handle)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan handles.
    pub(in crate::device) fn info(
        &self,
        handle: UiGlyphTextureHandle,
    ) -> Option<UiGlyphTextureResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|item| item.info)
    }

    /// Resolves a renderer-local identity to its live sampled image view.
    pub(in crate::device) fn view(&self, handle: UiGlyphTextureHandle) -> Option<vk::ImageView> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|item| item.image.view())
    }

    /// Releases views and images in reverse admission order.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.image.destroy(device, allocator);
        }
    }
}
