//! Typed identity and diagnostics for one immutable UI coverage atlas.

/// Stable renderer-local handle to one uploaded glyph atlas.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiGlyphTextureHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable allocation facts for one live coverage atlas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiGlyphTextureResourceInfo {
    identity: u64,
    extent: (u32, u32),
    upload_byte_count: usize,
}

impl UiGlyphTextureResourceInfo {
    pub(super) const fn new(identity: u64, extent: (u32, u32), upload_byte_count: usize) -> Self {
        Self {
            identity,
            extent,
            upload_byte_count,
        }
    }

    /// Returns the process-local CPU atlas generation identity.
    #[must_use]
    pub const fn identity(self) -> u64 {
        self.identity
    }

    /// Returns the uploaded coverage texture dimensions.
    #[must_use]
    pub const fn extent(self) -> (u32, u32) {
        self.extent
    }

    /// Returns the exact number of linear RGBA8 source bytes.
    #[must_use]
    pub const fn upload_byte_count(self) -> usize {
        self.upload_byte_count
    }
}
