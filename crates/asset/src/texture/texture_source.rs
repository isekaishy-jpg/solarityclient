//! Immutable parsed BLP sources retained in their authored compression.

use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

use super::{
    block_compression::{BlpBlockCompression, BlpBlockMip},
    blp::DecodedBlpTexture,
};

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

    /// Returns the authored DXT/BC family when GPU-ready blocks are retained.
    #[must_use]
    pub fn block_compression(&self) -> Option<BlpBlockCompression> {
        match self.image.compression_type() {
            wow_blp::CompressionType::Dxt1 => Some(BlpBlockCompression::Bc1),
            wow_blp::CompressionType::Dxt3 => Some(BlpBlockCompression::Bc2),
            wow_blp::CompressionType::Dxt5 => Some(BlpBlockCompression::Bc3),
            wow_blp::CompressionType::Jpeg
            | wow_blp::CompressionType::Raw1
            | wow_blp::CompressionType::Raw3 => None,
        }
    }

    /// Borrows one authored DXT mip without expanding it to RGBA8.
    ///
    /// The returned upload count is block-rounded. Some stock-compatible BLP
    /// tail mips retain fewer whole blocks than that count and must be padded
    /// with zeroes by the upload owner, matching the established decoder.
    #[must_use]
    pub fn block_mip(&self, mip_level: usize) -> Option<BlpBlockMip<'_>> {
        let compression = self.block_compression()?;
        let content = match compression {
            BlpBlockCompression::Bc1 => self.image.content_dxt1(),
            BlpBlockCompression::Bc2 => self.image.content_dxt3(),
            BlpBlockCompression::Bc3 => self.image.content_dxt5(),
        }?;
        let bytes = content.images.get(mip_level)?.content.as_slice();
        let (width, height) = self.mip_dimensions(mip_level)?;
        Some(BlpBlockMip::new(compression, width, height, bytes))
    }

    /// Returns the exact RGBA8 byte count for all authored mip levels.
    ///
    /// GPU upload uses this preflight to allocate one staging vector even for
    /// large same-path HD replacements. Every multiplication and accumulation
    /// is checked before decompression begins.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::TextureDecode`] when authored dimensions cannot be
    /// represented in the process address space.
    pub fn decoded_rgba8_byte_count(&self) -> Result<usize, AssetError> {
        let mut total = 0_usize;
        for mip_level in 0..self.mip_count() {
            let (width, height) = self.image.header.mipmap_size(mip_level);
            let byte_count = u64::from(width)
                .checked_mul(u64::from(height))
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .ok_or_else(|| AssetError::TextureDecode {
                    path: self.path.clone(),
                    message: format!(
                        "authored mip {mip_level} RGBA8 byte count overflows for {width}x{height}"
                    ),
                })?;
            total = total
                .checked_add(byte_count)
                .ok_or_else(|| AssetError::TextureDecode {
                    path: self.path.clone(),
                    message: "authored RGBA8 mip-chain byte count overflows".to_owned(),
                })?;
        }
        Ok(total)
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
