//! Placement-local character atlas upload and image lifetime ownership.

use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::device::vulkan_texture::{
    GpuSampledImage, Rgba8MipUpload, TextureUploadContext, upload_rgba8_mip_chain,
};
use crate::device::{BlpColorSpace, VulkanError};
use crate::model::CharacterAtlasTexture;

use super::types::{CharacterAtlasTextureHandle, CharacterAtlasTextureResourceInfo};

/// One device image and its immutable upload diagnostics.
struct GpuCharacterAtlasTexture {
    image: GpuSampledImage,
    info: CharacterAtlasTextureResourceInfo,
}

/// Owns placement-specific body atlases without assigning archive identities.
pub(in crate::device) struct CharacterAtlasTextureRegistry {
    registry_id: u64,
    resources: Vec<GpuCharacterAtlasTexture>,
    upload_submission_count: u64,
}

impl Default for CharacterAtlasTextureRegistry {
    /// Assigns process-unique renderer locality without allocating images.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            resources: Vec::new(),
            upload_submission_count: 0,
        }
    }
}

impl CharacterAtlasTextureRegistry {
    /// Uploads one complete stock-composed sRGB mip chain as a new placement image.
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        atlas: &CharacterAtlasTexture,
    ) -> Result<CharacterAtlasTextureHandle, VulkanError> {
        let (mips, byte_count) = validate_mips(atlas)?;
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::CharacterAtlasTextureCapacity)?;
        let image = upload_rgba8_mip_chain(context, &mips, BlpColorSpace::Srgb)?;
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
                (top.width(), top.height()),
                mips.len(),
                byte_count,
            ),
        });
        self.upload_submission_count = self.upload_submission_count.saturating_add(1);
        Ok(handle)
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

    /// Returns retired queue submissions spent admitting dynamic atlases.
    pub(in crate::device) const fn upload_submission_count(&self) -> u64 {
        self.upload_submission_count
    }

    /// Releases views and images in reverse placement-admission order.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        for mut resource in self.resources.drain(..).rev() {
            resource.image.destroy(device, allocator);
        }
    }
}

/// Verifies the private composer's fixed complete mip-chain invariant.
fn validate_mips(
    atlas: &CharacterAtlasTexture,
) -> Result<(Vec<Rgba8MipUpload<'_>>, usize), VulkanError> {
    const TOP_WIDTH: u32 = 256;
    const MIP_COUNT: usize = 9;

    if atlas.mips().len() != MIP_COUNT {
        return Err(VulkanError::operation(
            "validate character atlas",
            format!(
                "stock atlas requires {MIP_COUNT} mips; received {}",
                atlas.mips().len()
            ),
        ));
    }
    let mut expected_width = TOP_WIDTH;
    let mut byte_count = 0_usize;
    let mut uploads = Vec::with_capacity(MIP_COUNT);
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
