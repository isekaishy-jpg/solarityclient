//! Validated UI sampled-texture pairs and renderer-local descriptor identities.

use crate::device::{BlpTextureHandle, UiGlyphTextureHandle, UiSamplerHandle};

/// One typed sampled image accepted by the stock UI texture pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiTextureImageHandle {
    /// Archive-backed BLP image.
    Blp(BlpTextureHandle),
    /// Runtime-composed archive-font coverage atlas.
    Glyph(UiGlyphTextureHandle),
}

/// One uploaded BLP paired with its independently cached UI sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiSampledTexture {
    texture: UiTextureImageHandle,
    sampler: UiSamplerHandle,
}

impl UiSampledTexture {
    /// Joins image and sampler identities without exposing Vulkan handles.
    #[must_use]
    pub const fn new(texture: BlpTextureHandle, sampler: UiSamplerHandle) -> Self {
        Self {
            texture: UiTextureImageHandle::Blp(texture),
            sampler,
        }
    }

    /// Joins one glyph coverage atlas to an independently cached sampler.
    #[must_use]
    pub const fn glyph(texture: UiGlyphTextureHandle, sampler: UiSamplerHandle) -> Self {
        Self {
            texture: UiTextureImageHandle::Glyph(texture),
            sampler,
        }
    }

    /// Returns the renderer-local uploaded BLP identity.
    #[must_use]
    pub const fn texture(self) -> UiTextureImageHandle {
        self.texture
    }

    /// Returns the renderer-local UI sampler identity.
    #[must_use]
    pub const fn sampler(self) -> UiSamplerHandle {
        self.sampler
    }
}

/// Stable renderer-local handle to one persistent UI descriptor set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiTextureSetHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable image/sampler identity represented by a live descriptor set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiTextureSetInfo {
    sampled_texture: UiSampledTexture,
}

impl UiTextureSetInfo {
    pub(super) const fn new(sampled_texture: UiSampledTexture) -> Self {
        Self { sampled_texture }
    }

    /// Returns the exact image and sampler pair written to binding zero.
    #[must_use]
    pub const fn sampled_texture(self) -> UiSampledTexture {
        self.sampled_texture
    }
}
