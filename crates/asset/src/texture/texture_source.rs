//! Immutable parsed BLP sources retained in their authored compression.

use crate::{ArchiveDescriptor, AssetError, AssetNamespaceId, AssetPath, AssetStore};
use std::sync::Arc;

use super::{
    block_compression::{BlpBlockCompression, BlpBlockMip},
    blp::DecodedBlpTexture,
};

/// A selected archive BLP with compressed authored mip levels kept in memory.
///
/// Parsing once retains substantially less memory than eagerly expanding every
/// mip of an HD replacement to RGBA8. Consumers decode only the mip required by
/// character composition or GPU upload.
#[derive(Clone, Debug)]
pub struct BlpTextureSource {
    data: Arc<SourceData>,
}

#[derive(Debug)]
struct SourceData {
    namespace: AssetNamespaceId,
    path: AssetPath,
    archive: ArchiveDescriptor,
    image: wow_blp::BlpImage,
    resident_bytes: usize,
    // Drop retained allocations before releasing their one shared charge.
    storage: super::source_storage::SourceStorage,
}

impl BlpTextureSource {
    /// Resolves stock archive precedence and parses all authored BLP mip data.
    ///
    /// # Errors
    ///
    /// Returns archive lookup/read or admission pressure, or [`AssetError::TextureDecode`]
    /// when the selected bytes do not form a supported stock BLP image.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let _profile_scope = solarity_profiling::profile!("asset.texture.texture_source.load");
        let read = store.read(path)?;
        let archive = read.source().clone();
        let mut bytes = read.into_bytes();
        // Stock 0x4b5fe0 maps native pixel format 2 (BGRA8) and format 8
        // with eight alpha bits to the same output format. wow-blp 0.7 names
        // this header field AlphaType and omits 2 from its enum. Translate
        // only the equivalent BLP2/direct/RAW3 header; native format 2 does
        // not inspect alpha bits (sunGlare.blp carries 0x88 here). Retain all
        // authored pixel bytes, mip offsets, and ordinary parser validation.
        if bytes.starts_with(b"BLP2\x01\x00\x00\x00\x03") && bytes.get(10) == Some(&2) {
            bytes[10] = 8;
        }
        let metadata_bytes = size_of::<SourceData>()
            + 4 * size_of::<usize>()
            + path.as_str().len()
            + archive.owned_storage_bytes();
        let budget = store.effective_read_budget();
        let storage = super::source_storage::SourceStorage::admit(
            budget.as_ref(),
            super::source_storage::parse_bound(&bytes, metadata_bytes),
        )
        .map_err(|source| AssetError::ReadAdmission {
            asset: path.clone(),
            source,
        })?;
        let image = if let Some(image) = super::dxt_source::parse(path, &bytes)? {
            image
        } else {
            wow_blp::parser::load_blp_from_buf(&bytes).map_err(|error| {
                AssetError::TextureDecode {
                    path: path.clone(),
                    message: error.to_string(),
                }
            })?
        };

        let resident_bytes =
            metadata_bytes.saturating_add(super::source_storage::image_bytes(&image));
        if let Err(source) = storage.resize(resident_bytes) {
            drop(image);
            return Err(AssetError::ReadAdmission {
                asset: path.clone(),
                source,
            });
        }
        Ok(Self {
            data: Arc::new(SourceData {
                namespace: store.namespace(),
                path: path.clone(),
                archive,
                image,
                resident_bytes,
                storage,
            }),
        })
    }

    /// Returns the resource-declared top-mip width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.data.image.header.width
    }

    /// Returns the resource-declared top-mip height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.data.image.header.height
    }

    /// Returns the number of authored mip images available for decoding.
    #[must_use]
    pub fn mip_count(&self) -> usize {
        self.data.image.image_count()
    }

    /// Returns an authored mip's expected dimensions when that mip exists.
    #[must_use]
    pub fn mip_dimensions(&self, mip_level: usize) -> Option<(u32, u32)> {
        (mip_level < self.mip_count()).then(|| self.data.image.header.mipmap_size(mip_level))
    }

    /// Returns the authored DXT/BC family when GPU-ready blocks are retained.
    #[must_use]
    pub fn block_compression(&self) -> Option<BlpBlockCompression> {
        match self.data.image.compression_type() {
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
            BlpBlockCompression::Bc1 => self.data.image.content_dxt1(),
            BlpBlockCompression::Bc2 => self.data.image.content_dxt3(),
            BlpBlockCompression::Bc3 => self.data.image.content_dxt5(),
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
            let (width, height) = self.data.image.header.mipmap_size(mip_level);
            let byte_count = u64::from(width)
                .checked_mul(u64::from(height))
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .ok_or_else(|| AssetError::TextureDecode {
                    path: self.data.path.clone(),
                    message: format!(
                        "authored mip {mip_level} RGBA8 byte count overflows for {width}x{height}"
                    ),
                })?;
            total = total
                .checked_add(byte_count)
                .ok_or_else(|| AssetError::TextureDecode {
                    path: self.data.path.clone(),
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
        DecodedBlpTexture::from_image(
            &self.data.path,
            &self.data.archive,
            &self.data.image,
            mip_level,
        )
    }

    /// Returns the normalized archive path used as the texture cache identity.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.data.path
    }

    /// Returns the exact archive selected by stock patch priority.
    #[must_use]
    pub fn source(&self) -> &ArchiveDescriptor {
        &self.data.archive
    }

    /// Immutable archive selection that qualifies the virtual path at every cache layer.
    #[must_use]
    pub fn namespace(&self) -> AssetNamespaceId {
        self.data.namespace
    }

    /// Retained source capacity, including mip vectors, palette and source metadata.
    /// Shared path metadata is conservatively counted for each source generation.
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        self.data.resident_bytes
    }

    /// Admits a retained source when a CPU upload consumes it as required work.
    /// Existing speculative ownership transfers atomically; clones share one charge.
    ///
    /// # Errors
    /// Returns admission pressure without discarding the source or its original charge.
    pub fn admit_required(
        &self,
        budget: &solarity_cpu::CpuStorageBudget,
    ) -> Result<(), AssetError> {
        let policy =
            crate::AssetReadBudget::for_service(budget.clone(), solarity_cpu::CpuService::Required);
        self.data
            .storage
            .admit_for(Some(&policy), self.resident_bytes())
            .map_err(|source| AssetError::ReadAdmission {
                asset: self.path().clone(),
                source,
            })
    }

    /// Required consumers promote the shared charge; optional consumers never demote it.
    pub(crate) fn admit_for(&self, store: &AssetStore) -> Result<(), AssetError> {
        self.data
            .storage
            .admit_for(
                store.effective_read_budget().as_ref(),
                self.resident_bytes(),
            )
            .map_err(|source| AssetError::ReadAdmission {
                asset: self.path().clone(),
                source,
            })
    }
}
