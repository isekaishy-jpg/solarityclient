//! Explicit color interpretation, typed identity, and allocation diagnostics.

use solarity_asset::{AssetPath, BlpTextureSource};

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

/// Origin of one image admitted to the shared sampled-texture registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlpTextureSourceKind {
    /// Mips loaded from an authored BLP selected by archive precedence.
    Authored,
    /// Stock's opaque 8x8 green image for an empty WMO material stage.
    StockWorldModelGreen,
}

/// Device image storage selected from the authored BLP representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlpTextureStorage {
    /// Four uncompressed 8-bit color channels per texel.
    Rgba8,
    /// DXT1 blocks retained as Vulkan BC1.
    Bc1,
    /// DXT3 blocks retained as Vulkan BC2.
    Bc2,
    /// DXT5 blocks retained as Vulkan BC3.
    Bc3,
}

/// One ordered authored-texture admission request.
#[derive(Clone, Copy, Debug)]
pub struct BlpTextureUploadRequest<'source> {
    source: &'source BlpTextureSource,
    color_space: BlpColorSpace,
}

impl<'source> BlpTextureUploadRequest<'source> {
    /// Couples one selected archive source to its material-owned color space.
    #[must_use]
    pub const fn new(source: &'source BlpTextureSource, color_space: BlpColorSpace) -> Self {
        Self {
            source,
            color_space,
        }
    }

    /// Returns the parsed source retained by the asset cache.
    #[must_use]
    pub const fn source(self) -> &'source BlpTextureSource {
        self.source
    }

    /// Returns the requested sampling transfer function.
    #[must_use]
    pub const fn color_space(self) -> BlpColorSpace {
        self.color_space
    }
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
    source_kind: BlpTextureSourceKind,
    color_space: BlpColorSpace,
    storage: BlpTextureStorage,
    extent: (u32, u32),
    mip_count: usize,
    upload_byte_count: usize,
}

impl BlpTextureResourceInfo {
    /// Captures the exact image payload retained by one device allocation.
    pub(super) const fn new(
        path: AssetPath,
        source_kind: BlpTextureSourceKind,
        color_space: BlpColorSpace,
        storage: BlpTextureStorage,
        extent: (u32, u32),
        mip_count: usize,
        upload_byte_count: usize,
    ) -> Self {
        Self {
            path,
            source_kind,
            color_space,
            storage,
            extent,
            mip_count,
            upload_byte_count,
        }
    }

    /// Returns whether pixels came from an archive or a stock built-in image.
    #[must_use]
    pub const fn source_kind(&self) -> BlpTextureSourceKind {
        self.source_kind
    }

    /// Returns the normalized logical identity used for diagnostics.
    ///
    /// Authored images return their archive path. Built-in stock images return
    /// a reserved renderer identity and are distinguished by [`Self::source_kind`].
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the explicit sampling color interpretation.
    #[must_use]
    pub const fn color_space(&self) -> BlpColorSpace {
        self.color_space
    }

    /// Returns the exact uncompressed or BC storage family of the image.
    #[must_use]
    pub const fn storage(&self) -> BlpTextureStorage {
        self.storage
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

    /// Returns the total tightly packed staging payload size.
    #[must_use]
    pub const fn upload_byte_count(&self) -> usize {
        self.upload_byte_count
    }
}
