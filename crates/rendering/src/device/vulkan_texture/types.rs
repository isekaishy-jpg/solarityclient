//! Explicit color interpretation, typed identity, and allocation diagnostics.

use solarity_asset::AssetPath;

/// Caller-selected color interpretation for one stock texture role.
///
/// BLP does not encode a Vulkan color space. Requiring the owning material or
/// UI path to choose prevents a renderer-wide guessed gamma fallback.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlpColorSpace {
    /// Sample stored channel values without transfer conversion.
    Linear,
    /// Convert stored sRGB color channels to linear values while sampling.
    Srgb,
}

/// Stable renderer-local handle to one uploaded BLP image and view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlpTextureHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable dimensions and authored mip count of one live GPU image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlpTextureResourceInfo {
    path: AssetPath,
    color_space: BlpColorSpace,
    extent: (u32, u32),
    mip_count: usize,
    decoded_byte_count: usize,
}

impl BlpTextureResourceInfo {
    /// Captures the exact image payload retained by one device allocation.
    pub(super) const fn new(
        path: AssetPath,
        color_space: BlpColorSpace,
        extent: (u32, u32),
        mip_count: usize,
        decoded_byte_count: usize,
    ) -> Self {
        Self {
            path,
            color_space,
            extent,
            mip_count,
            decoded_byte_count,
        }
    }

    /// Returns the normalized archive path used for resource deduplication.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the explicit sampling color interpretation.
    #[must_use]
    pub const fn color_space(&self) -> BlpColorSpace {
        self.color_space
    }

    /// Returns the authored top-mip width and height.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Returns the number of authored mip levels uploaded.
    #[must_use]
    pub const fn mip_count(&self) -> usize {
        self.mip_count
    }

    /// Returns the total tightly packed RGBA8 staging payload size.
    #[must_use]
    pub const fn decoded_byte_count(&self) -> usize {
        self.decoded_byte_count
    }
}
