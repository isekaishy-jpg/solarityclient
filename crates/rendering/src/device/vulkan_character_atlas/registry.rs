//! Placement-local character atlas upload and image lifetime ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::device::vulkan_texture::{
    DeferredTextureTransfer, GpuSampledImage, Rgba8MipUpload, TextureUploadContext,
    upload_rgba8_mip_chain_deferred,
};
use crate::device::{BlpColorSpace, VulkanError};
use crate::model::{
    CharacterAtlasTexture, CharacterAtlasTextureKey, CharacterComponentTextureLevel,
};

use super::types::{CharacterAtlasTextureHandle, CharacterAtlasTextureResourceInfo};

/// One device image and its immutable upload diagnostics.
struct GpuCharacterAtlasTexture {
    image: GpuSampledImage,
    info: CharacterAtlasTextureResourceInfo,
}

/// Owns placement-specific body atlases without assigning archive identities.
pub(in crate::device) struct CharacterAtlasTextureRegistry {
    registry_id: u64,
    handles: HashMap<CharacterAtlasTextureKey, CharacterAtlasTextureHandle>,
    resources: Vec<GpuCharacterAtlasTexture>,
    pending_transfers: Vec<DeferredTextureTransfer>,
    upload_submission_count: u64,
}

impl Default for CharacterAtlasTextureRegistry {
    /// Assigns process-unique renderer locality without allocating images.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            pending_transfers: Vec::new(),
            upload_submission_count: 0,
        }
    }
}

impl CharacterAtlasTextureRegistry {
    /// Uploads one complete stock-composed mip chain as an M2 byte-space image.
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        atlas: &CharacterAtlasTexture,
    ) -> Result<CharacterAtlasTextureHandle, VulkanError> {
        self.retire_completed_transfers(context)?;
        if let Some(handle) = self.handles.get(atlas.key()) {
            return Ok(*handle);
        }
        let (mips, byte_count) = validate_mips(atlas)?;
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::CharacterAtlasTextureCapacity)?;
        // The atlas occupies an ordinary M2 texture stage after composition;
        // preserve the same fixed-function byte-space sampling as authored BLPs.
        let (image, transfer) =
            upload_rgba8_mip_chain_deferred(context, &mips, BlpColorSpace::Linear)?;
        let top = mips.first().ok_or_else(|| {
            VulkanError::operation("admit character atlas", "validated mip chain is empty")
        })?;
        let handle = CharacterAtlasTextureHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuCharacterAtlasTexture {
            image,
            info: CharacterAtlasTextureResourceInfo::new(
                BlpColorSpace::Linear,
                (top.width(), top.height()),
                mips.len(),
                byte_count,
            ),
        });
        self.pending_transfers.push(transfer);
        self.handles.insert(atlas.key().clone(), handle);
        self.upload_submission_count = self.upload_submission_count.saturating_add(1);
        Ok(handle)
    }

    /// Reclaims staging storage without waiting for unfinished GPU work.
    fn retire_completed_transfers(
        &mut self,
        context: TextureUploadContext<'_>,
    ) -> Result<(), VulkanError> {
        let mut index = self.pending_transfers.len();
        while index > 0 {
            index -= 1;
            if self.pending_transfers[index].is_complete(context.device)? {
                let mut transfer = self.pending_transfers.swap_remove(index);
                transfer.destroy(context.device, context.allocator);
            }
        }
        Ok(())
    }

    /// Returns stable allocation facts without exposing Vulkan handles.
    pub(in crate::device) fn info(
        &self,
        handle: CharacterAtlasTextureHandle,
    ) -> Option<CharacterAtlasTextureResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    /// Resolves a renderer-local identity to its live sampled view.
    pub(in crate::device) fn view(
        &self,
        handle: CharacterAtlasTextureHandle,
    ) -> Option<vk::ImageView> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.image.view())
    }

    /// Returns queue submissions spent admitting dynamic atlases.
    pub(in crate::device) const fn upload_submission_count(&self) -> u64 {
        self.upload_submission_count
    }

    /// Releases views and images in reverse placement-admission order.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for transfer in self.pending_transfers.iter_mut().rev() {
            transfer.destroy(device, allocator);
        }
        self.pending_transfers.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.image.destroy(device, allocator);
        }
    }
}

/// Verifies the private composer's selected complete mip-chain invariant.
fn validate_mips(
    atlas: &CharacterAtlasTexture,
) -> Result<(Vec<Rgba8MipUpload<'_>>, usize), VulkanError> {
    let top_width = atlas.mips().first().map(|mip| mip.width()).ok_or_else(|| {
        VulkanError::operation("validate character atlas", "character atlas is empty")
    })?;
    if top_width == 0 {
        return Err(VulkanError::operation(
            "validate character atlas",
            "character atlas top width is zero",
        ));
    }
    let component_level = CharacterComponentTextureLevel::new(top_width.ilog2() as u8)
        .filter(|level| level.atlas_size() == top_width)
        .ok_or_else(|| {
            VulkanError::operation(
                "validate character atlas",
                format!("unsupported character atlas top width {top_width}"),
            )
        })?;
    let mip_count = component_level.mip_count();
    if atlas.mips().len() != mip_count {
        return Err(VulkanError::operation(
            "validate character atlas",
            format!(
                "{top_width}-pixel stock atlas requires {mip_count} mips; received {}",
                atlas.mips().len()
            ),
        ));
    }
    let mut expected_width = top_width;
    let mut byte_count = 0_usize;
    let mut uploads = Vec::with_capacity(mip_count);
    for (expected_level, mip) in atlas.mips().iter().enumerate() {
        if mip.level() != expected_level || mip.width() != expected_width {
            return Err(VulkanError::operation(
                "validate character atlas",
                format!(
                    "mip {expected_level} requires width {expected_width}; received level {} width {}",
                    mip.level(),
                    mip.width()
                ),
            ));
        }
        let expected_bytes = u64::from(expected_width)
            .checked_mul(u64::from(expected_width))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or_else(|| {
                VulkanError::operation("validate character atlas", "mip byte count overflows")
            })?;
        if mip.rgba8().len() != expected_bytes {
            return Err(VulkanError::operation(
                "validate character atlas",
                format!(
                    "mip {expected_level} requires {expected_bytes} bytes; received {}",
                    mip.rgba8().len()
                ),
            ));
        }
        byte_count = byte_count.checked_add(expected_bytes).ok_or_else(|| {
            VulkanError::operation("validate character atlas", "total byte count overflows")
        })?;
        uploads.push(Rgba8MipUpload::new(
            expected_width,
            expected_width,
            mip.rgba8(),
        ));
        expected_width = (expected_width / 2).max(1);
    }
    Ok((uploads, byte_count))
}
