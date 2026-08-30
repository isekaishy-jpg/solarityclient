//! Stable asset-decoding and device-upload failures for BLP images.

use solarity_asset::AssetError;
use thiserror::Error;

use crate::device::VulkanError;

/// Failure to decode or upload one selected BLP source.
#[derive(Debug, Error)]
pub enum BlpTextureUploadError {
    /// One authored mip could not be decoded from the selected archive bytes.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Vulkan allocation, transfer, view creation, or synchronization failed.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
}
