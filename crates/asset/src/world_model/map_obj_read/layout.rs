//! Bounded chunk and scalar reads shared by strict root and group decoding.
use crate::{AssetError, AssetPath};

pub(super) fn read_u16(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<u16, AssetError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| world_model_message(path, format!("{field} is truncated")))?;
    Ok(u16::from_le_bytes(
        value
            .try_into()
            .map_err(|error| world_model_error(path, error))?,
    ))
}

pub(super) fn read_u32(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<u32, AssetError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| world_model_message(path, format!("{field} is truncated")))?;
    Ok(u32::from_le_bytes(
        value
            .try_into()
            .map_err(|error| world_model_error(path, error))?,
    ))
}

pub(super) fn read_f32(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<f32, AssetError> {
    Ok(f32::from_bits(read_u32(path, bytes, offset, field)?))
}

#[derive(Clone, Copy)]
pub(super) struct ChunkSpan {
    pub(super) magic: [u8; 4],
    pub(super) payload_start: usize,
    pub(super) payload_end: usize,
}

pub(super) fn scan_chunks(
    path: &AssetPath,
    bytes: &[u8],
    container: &str,
) -> Result<Vec<ChunkSpan>, AssetError> {
    let mut chunks = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8).ok_or_else(|| {
            world_model_message(path, format!("{container} chunk header offset overflows"))
        })?;
        let header = bytes.get(offset..header_end).ok_or_else(|| {
            world_model_message(path, format!("{container} ends inside a chunk header"))
        })?;
        let magic: [u8; 4] = header[0..4].try_into().map_err(|error| {
            world_model_error(
                path,
                format!("{container} chunk magic is truncated: {error}"),
            )
        })?;
        let size = usize::try_from(u32::from_le_bytes(header[4..8].try_into().map_err(
            |error| {
                world_model_error(
                    path,
                    format!("{container} chunk size is truncated: {error}"),
                )
            },
        )?))
        .map_err(|error| world_model_error(path, error))?;
        let payload_end = header_end.checked_add(size).ok_or_else(|| {
            world_model_message(path, format!("{container} chunk extent overflows"))
        })?;
        if payload_end > bytes.len() {
            return Err(world_model_message(
                path,
                format!("{container} chunk exceeds its containing payload"),
            ));
        }
        chunks.push(ChunkSpan {
            magic,
            payload_start: header_end,
            payload_end,
        });
        offset = payload_end;
    }
    Ok(chunks)
}

pub(super) fn validate_unique_chunks(
    path: &AssetPath,
    chunks: &[ChunkSpan],
    allowed: &[[u8; 4]],
    container: &str,
    repeatable: &[[u8; 4]],
) -> Result<(), AssetError> {
    let mut seen = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if !allowed.contains(&chunk.magic) {
            return Err(world_model_message(
                path,
                format!(
                    "{container} contains unknown build-12340 chunk {}",
                    chunk_name(chunk.magic)
                ),
            ));
        }
        if seen.contains(&chunk.magic) && !repeatable.contains(&chunk.magic) {
            return Err(world_model_message(
                path,
                format!("{container} repeats chunk {}", chunk_name(chunk.magic)),
            ));
        }
        seen.push(chunk.magic);
    }
    Ok(())
}

pub(super) fn require_chunk<'a>(
    path: &AssetPath,
    chunks: &'a [ChunkSpan],
    magic: [u8; 4],
    name: &str,
) -> Result<&'a ChunkSpan, AssetError> {
    chunks
        .iter()
        .find(|chunk| chunk.magic == magic)
        .ok_or_else(|| world_model_message(path, format!("WMO omits required {name}")))
}

pub(super) fn chunk_name(mut magic: [u8; 4]) -> String {
    magic.reverse();
    String::from_utf8_lossy(&magic).into_owned()
}

pub(super) fn validate_bounds(
    path: &AssetPath,
    minimum: [f32; 3],
    maximum: [f32; 3],
    table: &str,
) -> Result<[[f32; 3]; 2], AssetError> {
    if minimum
        .into_iter()
        .chain(maximum)
        .any(|value| !value.is_finite())
        || (0..3).any(|axis| minimum[axis] > maximum[axis])
    {
        return Err(world_model_message(
            path,
            format!("{table} has invalid bounds"),
        ));
    }
    Ok([minimum, maximum])
}

pub(super) fn world_model_error(path: &AssetPath, error: impl std::fmt::Display) -> AssetError {
    world_model_message(path, error.to_string())
}

pub(super) fn world_model_message(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::WorldModelDecode {
        path: path.clone(),
        message: message.into(),
    }
}
