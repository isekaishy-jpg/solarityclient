//! Typed UI sampler identity and observable axis-addressing state.

use crate::UiTextureAddressMode;

/// Stable renderer-local handle to one Vulkan UI sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiSamplerHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Complete immutable addressing state represented by a live UI sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiSamplerInfo {
    horizontal: UiTextureAddressMode,
    vertical: UiTextureAddressMode,
}

impl UiSamplerInfo {
    /// Captures both independently authored tiling axes.
    #[must_use]
    pub const fn new(horizontal: UiTextureAddressMode, vertical: UiTextureAddressMode) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    /// Returns the U-axis addressing operation.
    #[must_use]
    pub const fn horizontal(self) -> UiTextureAddressMode {
        self.horizontal
    }

    /// Returns the V-axis addressing operation.
    #[must_use]
    pub const fn vertical(self) -> UiTextureAddressMode {
        self.vertical
    }
}
