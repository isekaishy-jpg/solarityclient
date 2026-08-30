//! Immutable parsed BLP sources retained in their authored compression.

use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

use super::blp::DecodedBlpTexture;

/// A selected archive BLP with compressed authored mip levels kept in memory.
///
/// Parsing once retains substantially less memory than eagerly expanding every
/// mip of an HD replacement to RGBA8. Consumers decode only the mip required by
/// character composition or GPU upload.
#[derive(Debug)]
pub struct BlpTextureSource {
    path: AssetPath,
    archive: ArchiveDescriptor,
    image: wow_blp::BlpImage,
}

impl BlpTextureSource {
    /// Resolves stock archive precedence and parses all authored BLP mip data.
    ///
    /// # Errors
    ///
    /// Returns an archive lookup/read error or [`AssetError::TextureDecode`]
    /// when the selected bytes do not form a supported stock BLP image.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        let image = wow_blp::parser::load_blp_from_buf(read.bytes()).map_err(|error| {
            AssetError::TextureDecode {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            path: path.clone(),
            archive: read.source().clone(),
            image,
        })
    }

    /// Returns the resource-declared top-mip width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.image.header.width
    }

    /// Returns the resource-declared top-mip height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.image.header.height
    }

    /// Returns the number of authored mip images available for decoding.
    #[must_use]
    pub fn mip_count(&self) -> usize {
        self.image.image_count()
    }

    /// Returns an authored mip's expected dimensions when that mip exists.
    #[must_use]
    pub fn mip_dimensions(&self, mip_level: usize) -> Option<(u32, u32)> {
        (mip_level < self.mip_count()).then(|| self.image.header.mipmap_size(mip_level))
    }

    /// Decodes one authored mip to tightly packed row-major RGBA8 pixels.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::TextureDecode`] when the requested level is absent
    /// or its retained compressed pixels cannot be converted.
    pub fn decode_mip(&self, mip_level: usize) -> Result<DecodedBlpTexture, AssetError> {
        DecodedBlpTexture::from_image(&self.path, &self.archive, &self.image, mip_level)
    }

    /// Returns the normalized archive path used as the texture cache identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the exact archive selected by stock patch priority.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.archive
    }
}
