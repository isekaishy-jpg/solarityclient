//! Strict build-12340 MCAL expansion into one GPU-ready RGBA map.

use super::{TerrainChunkIndex, TerrainTextureLayer};

/// Width and height of each authored terrain alpha plane.
pub const TERRAIN_ALPHA_MAP_WIDTH: usize = 64;

/// Byte count of the combined three-layer RGBA upload payload.
pub const TERRAIN_ALPHA_MAP_BYTE_COUNT: usize =
    TERRAIN_ALPHA_MAP_WIDTH * TERRAIN_ALPHA_MAP_WIDTH * 4;

const ALPHA_PLANE_TEXELS: usize = TERRAIN_ALPHA_MAP_WIDTH * TERRAIN_ALPHA_MAP_WIDTH;
const SMALL_ALPHA_PLANE_BYTES: usize = ALPHA_PLANE_TEXELS / 2;
const USE_ALPHA_MAP: u32 = 0x100;
const COMPRESSED_ALPHA_MAP: u32 = 0x200;
const DO_NOT_FIX_ALPHA_MAP: u32 = 0x8000;

/// Three terrain blend planes packed into RGB; alpha remains opaque.
pub struct TerrainAlphaMap {
    rgba: Box<[u8; TERRAIN_ALPHA_MAP_BYTE_COUNT]>,
}

impl TerrainAlphaMap {
    /// Returns the stable 64-by-64 RGBA8 upload payload.
    #[must_use]
    pub fn rgba(&self) -> &[u8; TERRAIN_ALPHA_MAP_BYTE_COUNT] {
        &self.rgba
    }
}

pub(super) fn decode_alpha_map(
    chunk: TerrainChunkIndex,
    chunk_flags: u32,
    layers: &[TerrainTextureLayer],
    bytes: &[u8],
    big_alpha: bool,
) -> Result<Option<TerrainAlphaMap>, String> {
    if layers.len() == 1 {
        return Ok(None);
    }
    let mut rgba = Box::new([0_u8; TERRAIN_ALPHA_MAP_BYTE_COUNT]);
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[3] = u8::MAX;
    }
    for layer_index in 1..layers.len() {
        let layer = layers[layer_index];
        if layer.flags() & USE_ALPHA_MAP == 0 {
            return Err(format!(
                "MCNK {chunk:?} blend layer {layer_index} omits the alpha-map flag"
            ));
        }
        let start = layer.alpha_offset() as usize;
        let end = layers
            .get(layer_index + 1)
            .map_or(bytes.len(), |next| next.alpha_offset() as usize);
        if start > end || end > bytes.len() {
            return Err(format!(
                "MCNK {chunk:?} layer {layer_index} alpha range {start}..{end} exceeds {} bytes",
                bytes.len()
            ));
        }
        let source = &bytes[start..end];
        let mut plane = if layer.flags() & COMPRESSED_ALPHA_MAP != 0 {
            decode_rle(chunk, layer_index, source)?
        } else if big_alpha {
            decode_big(chunk, layer_index, source)?
        } else {
            decode_small(chunk, layer_index, source)?
        };
        if chunk_flags & DO_NOT_FIX_ALPHA_MAP == 0 {
            fix_alpha_edges(&mut plane);
        }
        let channel = layer_index - 1;
        for (pixel, alpha) in rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(plane.iter().copied())
        {
            pixel[channel] = alpha;
        }
    }
    Ok(Some(TerrainAlphaMap { rgba }))
}

fn decode_big(
    chunk: TerrainChunkIndex,
    layer: usize,
    source: &[u8],
) -> Result<Box<[u8; ALPHA_PLANE_TEXELS]>, String> {
    let values = source.get(..ALPHA_PLANE_TEXELS).ok_or_else(|| {
        format!(
            "MCNK {chunk:?} layer {layer} requires {ALPHA_PLANE_TEXELS} big-alpha bytes; found {}",
            source.len()
        )
    })?;
    values
        .to_vec()
        .into_boxed_slice()
        .try_into()
        .map_err(|values: Box<[u8]>| {
            format!(
                "MCNK {chunk:?} layer {layer} decoded {} big-alpha texels",
                values.len()
            )
        })
}

fn decode_small(
    chunk: TerrainChunkIndex,
    layer: usize,
    source: &[u8],
) -> Result<Box<[u8; ALPHA_PLANE_TEXELS]>, String> {
    let packed = source.get(..SMALL_ALPHA_PLANE_BYTES).ok_or_else(|| {
        format!(
            "MCNK {chunk:?} layer {layer} requires {SMALL_ALPHA_PLANE_BYTES} small-alpha bytes; found {}",
            source.len()
        )
    })?;
    let mut values = Box::new([0_u8; ALPHA_PLANE_TEXELS]);
    for (destination, value) in values.as_chunks_mut::<2>().0.iter_mut().zip(packed) {
        let low = value & 0x0F;
        let high = value >> 4;
        destination[0] = low | (low << 4);
        destination[1] = high | (high << 4);
    }
    Ok(values)
}

fn decode_rle(
    chunk: TerrainChunkIndex,
    layer: usize,
    source: &[u8],
) -> Result<Box<[u8; ALPHA_PLANE_TEXELS]>, String> {
    let mut output = Vec::with_capacity(ALPHA_PLANE_TEXELS);
    let mut offset = 0;
    while output.len() < ALPHA_PLANE_TEXELS {
        let token = *source.get(offset).ok_or_else(|| {
            format!(
                "MCNK {chunk:?} layer {layer} compressed alpha ends at {} of {ALPHA_PLANE_TEXELS} texels",
                output.len()
            )
        })?;
        offset += 1;
        let count = usize::from(token & 0x7F);
        if count == 0 {
            return Err(format!(
                "MCNK {chunk:?} layer {layer} compressed alpha contains a zero-length run"
            ));
        }
        if output.len() + count > ALPHA_PLANE_TEXELS {
            return Err(format!(
                "MCNK {chunk:?} layer {layer} compressed alpha exceeds {ALPHA_PLANE_TEXELS} texels"
            ));
        }
        if token & 0x80 != 0 {
            let value = *source.get(offset).ok_or_else(|| {
                format!("MCNK {chunk:?} layer {layer} compressed fill omits its value")
            })?;
            offset += 1;
            output.resize(output.len() + count, value);
        } else {
            let end = offset.checked_add(count).ok_or_else(|| {
                format!("MCNK {chunk:?} layer {layer} compressed literal range overflows")
            })?;
            let values = source.get(offset..end).ok_or_else(|| {
                format!(
                    "MCNK {chunk:?} layer {layer} compressed literal of {count} bytes is truncated"
                )
            })?;
            output.extend_from_slice(values);
            offset = end;
        }
    }
    output
        .into_boxed_slice()
        .try_into()
        .map_err(|values: Box<[u8]>| {
            format!(
                "MCNK {chunk:?} layer {layer} decoded {} compressed-alpha texels",
                values.len()
            )
        })
}

fn fix_alpha_edges(values: &mut [u8; ALPHA_PLANE_TEXELS]) {
    for row in 0..TERRAIN_ALPHA_MAP_WIDTH - 1 {
        let final_column = row * TERRAIN_ALPHA_MAP_WIDTH + TERRAIN_ALPHA_MAP_WIDTH - 1;
        values[final_column] = values[final_column - 1];
    }
    let previous_row = (TERRAIN_ALPHA_MAP_WIDTH - 2) * TERRAIN_ALPHA_MAP_WIDTH;
    let final_row = (TERRAIN_ALPHA_MAP_WIDTH - 1) * TERRAIN_ALPHA_MAP_WIDTH;
    values.copy_within(
        previous_row..previous_row + TERRAIN_ALPHA_MAP_WIDTH,
        final_row,
    );
}
