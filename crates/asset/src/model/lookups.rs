//! Exact build-12340 M2 model lookup-table ownership.

use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// Small header tables referenced by SKIN batches and replacement systems.
#[derive(Debug)]
pub(super) struct M2LookupTables {
    pub(super) replaceable_textures: Vec<u16>,
    pub(super) bones: Vec<u16>,
    pub(super) textures: Vec<u16>,
    pub(super) texture_coordinates: Vec<i16>,
    pub(super) texture_weights: Vec<u16>,
    pub(super) texture_transforms: Vec<u16>,
}

impl M2LookupTables {
    /// Decodes and validates every fixed-width version-264 lookup directly.
    pub(super) fn decode(
        path: &AssetPath,
        bytes: &[u8],
        bone_count: usize,
        texture_count: usize,
        texture_weight_count: usize,
        texture_transform_count: usize,
    ) -> Result<Self, AssetError> {
        Ok(Self {
            replaceable_textures: decode_optional_lookup(
                path,
                bytes,
                0x68,
                "replaceable-texture lookup",
                texture_count,
            )?,
            bones: decode_required_lookup(path, bytes, 0x78, "bone lookup", bone_count)?,
            textures: decode_required_lookup(path, bytes, 0x80, "texture lookup", texture_count)?,
            texture_coordinates: decode_signed_lookup(
                path,
                bytes,
                0x88,
                "texture-coordinate lookup",
            )?,
            texture_weights: decode_optional_lookup(
                path,
                bytes,
                0x90,
                "texture-weight lookup",
                texture_weight_count,
            )?,
            texture_transforms: decode_optional_lookup(
                path,
                bytes,
                0x98,
                "texture-transform lookup",
                texture_transform_count,
            )?,
        })
    }
}

/// Decodes a table whose every entry must name an existing record.
fn decode_required_lookup(
    path: &AssetPath,
    bytes: &[u8],
    pair_offset: usize,
    field: &str,
    target_count: usize,
) -> Result<Vec<u16>, AssetError> {
    let array = array_ref(path, bytes, pair_offset, field)?;
    validate_array(path, bytes, array, 2, field)?;
    let mut values = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let value = read_u16(path, bytes, array.offset + index * 2, field)?;
        if usize::from(value) >= target_count {
            return Err(model_decode(
                path,
                format!("{field} {index} references missing entry {value}"),
            ));
        }
        values.push(value);
    }
    Ok(values)
}

/// Decodes a table where `0xFFFF` is the only absent-entry sentinel.
fn decode_optional_lookup(
    path: &AssetPath,
    bytes: &[u8],
    pair_offset: usize,
    field: &str,
    target_count: usize,
) -> Result<Vec<u16>, AssetError> {
    let array = array_ref(path, bytes, pair_offset, field)?;
    validate_array(path, bytes, array, 2, field)?;
    let mut values = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let value = read_u16(path, bytes, array.offset + index * 2, field)?;
        if value != u16::MAX && usize::from(value) >= target_count {
            return Err(model_decode(
                path,
                format!("{field} {index} references missing entry {value}"),
            ));
        }
        values.push(value);
    }
    Ok(values)
}

/// Preserves signed texture-coordinate selectors without interpreting effects.
fn decode_signed_lookup(
    path: &AssetPath,
    bytes: &[u8],
    pair_offset: usize,
    field: &str,
) -> Result<Vec<i16>, AssetError> {
    let array = array_ref(path, bytes, pair_offset, field)?;
    validate_array(path, bytes, array, 2, field)?;
    (0..array.count)
        .map(|index| read_i16(path, bytes, array.offset + index * 2, field))
        .collect()
}

#[derive(Clone, Copy)]
struct ArrayRef {
    count: usize,
    offset: usize,
}

fn array_ref(
    path: &AssetPath,
    bytes: &[u8],
    pair_offset: usize,
    field: &str,
) -> Result<ArrayRef, AssetError> {
    Ok(ArrayRef {
        count: read_u32(path, bytes, pair_offset, field)? as usize,
        offset: read_u32(path, bytes, pair_offset + 4, field)? as usize,
    })
}

fn validate_array(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    stride: usize,
    field: &str,
) -> Result<(), AssetError> {
    let byte_count = array
        .count
        .checked_mul(stride)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    let end = array
        .offset
        .checked_add(byte_count)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    if end > bytes.len() {
        return Err(model_decode(
            path,
            format!("{field} array exceeds the M2 file"),
        ));
    }
    Ok(())
}

fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u32, AssetError> {
    Ok(u32::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_u16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u16, AssetError> {
    Ok(u16::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_i16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<i16, AssetError> {
    Ok(i16::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_bytes<const N: usize>(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<[u8; N], AssetError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| model_decode(path, format!("{field} is truncated")))
}
