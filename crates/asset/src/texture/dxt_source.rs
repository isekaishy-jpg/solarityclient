//! Bounded BLP2/DXT payload parsing, including authored narrow tail mips.

use super::block_compression::BlpBlockCompression;
use crate::{AssetError, AssetPath};
use wow_blp::{
    AlphaType, BlpContent, BlpContentTag, BlpDxtn, BlpFlags, BlpHeader, BlpImage, BlpVersion,
    Compression, DxtnFormat, DxtnImage, MipmapLocator,
};

/// Returns `None` for encodings owned by the general BLP parser.
pub(super) fn parse(path: &AssetPath, bytes: &[u8]) -> Result<Option<BlpImage>, AssetError> {
    if !bytes.starts_with(b"BLP2\x01\0\0\0\x02") {
        return Ok(None);
    }
    let fail = |message: String| AssetError::TextureDecode {
        path: path.clone(),
        message,
    };
    if bytes.len() < 148 {
        return Err(fail("truncated BLP2/DXT header".to_owned()));
    }
    let word = |offset| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let width = word(12);
    let height = word(16);
    if !(1..=65535).contains(&width) || !(1..=65535).contains(&height) {
        return Err(fail(format!("invalid BLP2/DXT extent {width}x{height}")));
    }
    let (compression, format, alpha_type) = match bytes[10] {
        0 => (BlpBlockCompression::Bc1, DxtnFormat::Dxt1, AlphaType::None),
        1 => (
            BlpBlockCompression::Bc2,
            DxtnFormat::Dxt3,
            AlphaType::OneBit,
        ),
        7 => (
            BlpBlockCompression::Bc3,
            DxtnFormat::Dxt5,
            AlphaType::Enhanced,
        ),
        value => return Err(fail(format!("unsupported BLP2/DXT pixel format {value}"))),
    };
    let offsets = std::array::from_fn(|index| word(20 + index * 4));
    let sizes = std::array::from_fn(|index| word(84 + index * 4));
    let header = BlpHeader {
        version: BlpVersion::Blp2,
        content: BlpContentTag::Direct,
        flags: BlpFlags::Blp2 {
            compression: Compression::Dxtc,
            alpha_bits: bytes[9],
            alpha_type,
            has_mipmaps: bytes[11],
        },
        width,
        height,
        mipmap_locator: MipmapLocator::Internal { offsets, sizes },
    };
    // Stock 6AB700 halves each axis independently down to one. Parse only
    // populated internal levels; never invent offsets for an absent tail.
    let mip_count = if bytes[11] & 0x0f == 0 {
        1
    } else {
        (width.max(height).ilog2() + 1).min(16) as usize
    };
    let mut images = Vec::with_capacity(mip_count);
    for mip in 0..mip_count {
        let offset = offsets[mip] as usize;
        let size = sizes[mip] as usize;
        if mip > 0 && offset == 0 && size == 0 {
            break;
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| fail(format!("DXT mip {mip} range overflows")))?;
        let payload = bytes
            .get(offset..end)
            .filter(|_| offset >= 148 && size > 0)
            .ok_or_else(|| {
                fail(format!(
                    "DXT mip {mip} range {offset}..{end} is outside the file payload"
                ))
            })?;
        let (width, height) = header.mipmap_size(mip);
        let expected = compression.mip_byte_count(width, height);
        let block_size = compression.block_byte_count();
        // Installed 12340 narrow tails store one whole block once an axis
        // falls below four. Retain that authored payload; CPU decode and GPU
        // upload share the existing zero-padding boundary for missing blocks.
        // Larger incomplete images and partial blocks are actual decode errors.
        let narrow_tail = mip > 0 && width.min(height) < 4 && size == block_size;
        if !size.is_multiple_of(block_size) || (size < expected && !narrow_tail) {
            return Err(fail(format!(
                "DXT mip {mip} ({width}x{height}) has {size} bytes; expected {expected} or one complete narrow-tail block"
            )));
        }
        // Round the axes separately. ceil(width*height/16), used by wow-blp
        // 0.7, silently drops valid blocks from complete narrow/odd extents.
        images.push(DxtnImage {
            content: payload[..size.min(expected)].to_vec(),
        });
    }
    let content = BlpDxtn {
        format,
        cmap: Vec::new(),
        images,
    };
    let content = match compression {
        BlpBlockCompression::Bc1 => BlpContent::Dxt1(content),
        BlpBlockCompression::Bc2 => BlpContent::Dxt3(content),
        BlpBlockCompression::Bc3 => BlpContent::Dxt5(content),
    };
    Ok(Some(BlpImage { header, content }))
}
