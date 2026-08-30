//! Stock BLP decoding into an owned CPU-side RGBA8 texture.

use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

/// Top-level BLP mip decoded to tightly packed row-major RGBA8 pixels.
#[derive(Clone, Debug)]
pub struct DecodedBlpTexture {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
    source: ArchiveDescriptor,
}

impl DecodedBlpTexture {
    /// Resolves stock archive precedence and decodes mip level zero.
    ///
    /// Dimensions come from the BLP header, so HD replacements naturally
    /// allocate their larger resource size without a separate code path.
    ///
    /// # Errors
    ///
    /// Returns an archive lookup/read error or [`AssetError::TextureDecode`]
    /// when the selected bytes cannot produce a complete RGBA8 top mip.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        let blp = wow_blp::parser::load_blp_from_buf(read.bytes()).map_err(|source| {
            AssetError::TextureDecode {
                path: path.clone(),
                message: source.to_string(),
            }
        })?;
        let image = wow_blp::convert::blp_to_image(&blp, 0).map_err(|source| {
            AssetError::TextureDecode {
                path: path.clone(),
                message: source.to_string(),
            }
        })?;
        let rgba8 = image.into_rgba8();
        let (width, height) = rgba8.dimensions();

        Ok(Self {
            width,
            height,
            rgba8: rgba8.into_raw(),
            source: read.source().clone(),
        })
    }

    /// Returns the resource-declared top-mip width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the resource-declared top-mip height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns tightly packed row-major red, green, blue, and alpha bytes.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Transfers pixel ownership to an upload stage without copying.
    #[must_use]
    pub fn into_rgba8(self) -> Vec<u8> {
        self.rgba8
    }

    /// Returns the exact archive selected by stock patch priority.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }
}
