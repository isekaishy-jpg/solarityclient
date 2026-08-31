//! Typed renderer identity and diagnostics for one composed body atlas.

/// Stable renderer-local handle to one placement-owned character atlas image.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CharacterAtlasTextureHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable allocation facts for one complete dynamic body texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterAtlasTextureResourceInfo {
    extent: (u32, u32),
    mip_count: usize,
    upload_byte_count: usize,
}

impl CharacterAtlasTextureResourceInfo {
    /// Captures the exact composed mip payload admitted to the device.
    pub(super) const fn new(
        extent: (u32, u32),
        mip_count: usize,
        upload_byte_count: usize,
    ) -> Self {
        Self {
            extent,
            mip_count,
            upload_byte_count,
        }
    }

    /// Returns the top-level atlas width and height.
    #[must_use]
    pub const fn extent(self) -> (u32, u32) {
        self.extent
    }

    /// Returns the number of composed mip levels uploaded.
    #[must_use]
    pub const fn mip_count(self) -> usize {
        self.mip_count
    }

    /// Returns the total tightly packed RGBA8 upload size.
    #[must_use]
    pub const fn upload_byte_count(self) -> usize {
        self.upload_byte_count
    }
}
