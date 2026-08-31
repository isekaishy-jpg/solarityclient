//! Path/color-space deduplication and image lifetime ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::{AssetPath, BlpTextureSource};

use super::status::BlpTextureUploadError;
use super::types::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureUploadRequest,
};
use super::upload::{
    GpuBlpTexture, TextureUploadContext, upload_stock_world_model_green, upload_textures,
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
    upload_submission_count: u64,
}

impl Default for BlpTextureRegistry {
    /// Assigns process-unique renderer locality without allocating images.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            upload_submission_count: 0,
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
        self.upload_batch(
            context,
            &[BlpTextureUploadRequest::new(source, color_space)],
        )?
        .pop()
        .ok_or_else(|| {
            crate::device::VulkanError::operation(
                "finish BLP registry upload",
                "handle is unavailable",
            )
            .into()
        })
    }

    /// Preserves request order while admitting every new identity together.
    pub(in crate::device) fn upload_batch(
        &mut self,
        context: TextureUploadContext<'_>,
        requests: &[BlpTextureUploadRequest<'_>],
    ) -> Result<Vec<BlpTextureHandle>, BlpTextureUploadError> {
        let mut pending_by_key = HashMap::<BlpTextureKey, usize>::new();
        let mut pending_keys = Vec::new();
        let mut pending_requests = Vec::new();
        for request in requests {
            let key = BlpTextureKey::Authored {
                path: request.source().path().clone(),
                color_space: request.color_space(),
            };
            if self.handles.contains_key(&key) || pending_by_key.contains_key(&key) {
                continue;
            }
            pending_by_key.insert(key.clone(), pending_keys.len());
            pending_keys.push(key);
            pending_requests.push((request.source(), request.color_space()));
        }

        if !pending_requests.is_empty() {
            let final_len = self
                .resources
                .len()
                .checked_add(pending_requests.len())
                .ok_or(crate::device::VulkanError::BlpTextureCapacity)?;
            u32::try_from(final_len - 1)
                .map_err(|_source| crate::device::VulkanError::BlpTextureCapacity)?;
            let mut resources = upload_textures(context, &pending_requests)?;
            if resources.len() != pending_keys.len() {
                for resource in resources.iter_mut().rev() {
                    resource.destroy(context.device, context.allocator);
                }
                return Err(crate::device::VulkanError::operation(
                    "finish BLP batch upload",
                    "resource count does not match admitted identities",
                )
                .into());
            }
            for (key, resource) in pending_keys.into_iter().zip(resources) {
                let slot = u32::try_from(self.resources.len())
                    .map_err(|_source| crate::device::VulkanError::BlpTextureCapacity)?;
                let handle = BlpTextureHandle {
                    registry_id: self.registry_id,
                    slot,
                };
                self.resources.push(resource);
                self.handles.insert(key, handle);
            }
            self.upload_submission_count = self.upload_submission_count.saturating_add(1);
        }

        requests
            .iter()
            .map(|request| {
                let key = BlpTextureKey::Authored {
                    path: request.source().path().clone(),
                    color_space: request.color_space(),
                };
                self.handles.get(&key).copied().ok_or_else(|| {
                    crate::device::VulkanError::operation(
                        "resolve BLP batch handle",
                        "admitted identity is unavailable",
                    )
                    .into()
                })
            })
            .collect()
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
        self.upload_submission_count = self.upload_submission_count.saturating_add(1);
        Ok(handle)
    }

    /// Returns the number of retired queue submissions used for texture admission.
    pub(in crate::device) const fn upload_submission_count(&self) -> u64 {
        self.upload_submission_count
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
