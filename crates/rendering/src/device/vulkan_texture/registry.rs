//! Path/color-space deduplication and image lifetime ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::{AssetPath, BlpTextureSource};

use super::status::BlpTextureUploadError;
use super::types::{BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo};
use super::upload::{
    GpuBlpTexture, TextureUploadContext, upload_stock_world_model_green, upload_texture,
};

/// Image identity includes color interpretation because it fixes VkFormat.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum BlpTextureKey {
    Authored {
        path: AssetPath,
        color_space: BlpColorSpace,
    },
    StockWorldModelGreen,
}

/// Owns every uploaded BLP image until the parent renderer is torn down.
pub(in crate::device) struct BlpTextureRegistry {
    registry_id: u64,
    handles: HashMap<BlpTextureKey, BlpTextureHandle>,
    resources: Vec<GpuBlpTexture>,
}

impl Default for BlpTextureRegistry {
    /// Assigns process-unique renderer locality without allocating images.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl BlpTextureRegistry {
    /// Returns an existing identity or uploads every authored mip once.
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        source: &BlpTextureSource,
        color_space: BlpColorSpace,
    ) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let key = BlpTextureKey::Authored {
            path: source.path().clone(),
            color_space,
        };
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| crate::device::VulkanError::BlpTextureCapacity)?;
        let resource = upload_texture(context, source, color_space)?;
        let handle = BlpTextureHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(resource);
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Returns or creates stock's one renderer-local WMO placeholder image.
    pub(in crate::device) fn upload_stock_world_model_green(
        &mut self,
        context: TextureUploadContext<'_>,
    ) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let key = BlpTextureKey::StockWorldModelGreen;
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| crate::device::VulkanError::BlpTextureCapacity)?;
        let resource = upload_stock_world_model_green(context)?;
        let handle = BlpTextureHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(resource);
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan handles.
    pub(in crate::device) fn info(
        &self,
        handle: BlpTextureHandle,
    ) -> Option<&BlpTextureResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(GpuBlpTexture::info)
    }

    /// Resolves a renderer-local identity to its live sampled image view.
    pub(in crate::device) fn view(&self, handle: BlpTextureHandle) -> Option<ash::vk::ImageView> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(GpuBlpTexture::view)
    }

    /// Releases views and images in reverse upload order before VMA teardown.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.destroy(device, allocator);
        }
    }
}
