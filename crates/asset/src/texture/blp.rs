//! Stock BLP mip decoding into an owned CPU-side RGBA8 texture.

use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

use super::texture_source::BlpTextureSource;

/// One BLP mip decoded to tightly packed row-major RGBA8 pixels.
#[derive(Clone, Debug)]
pub struct DecodedBlpTexture {
    mip_level: usize,
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
        BlpTextureSource::load(store, path)?.decode_mip(0)
    }

    /// Builds one decoded mip while preserving its selected archive identity.
    pub(crate) fn from_image(
        path: &AssetPath,
        source: &ArchiveDescriptor,
        blp: &wow_blp::BlpImage,
        mip_level: usize,
    ) -> Result<Self, AssetError> {
        let image = wow_blp::convert::blp_to_image(blp, mip_level).map_err(|error| {
            AssetError::TextureDecode {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;
        let rgba8 = image.into_rgba8();
        let (width, height) = rgba8.dimensions();

        Ok(Self {
            mip_level,
            width,
            height,
            rgba8: rgba8.into_raw(),
            source: source.clone(),
        })
    }

    /// Returns the authored BLP mip level represented by these pixels.
    #[must_use]
    pub const fn mip_level(&self) -> usize {
        self.mip_level
    }

    /// Returns this decoded mip's width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns this decoded mip's height.
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
