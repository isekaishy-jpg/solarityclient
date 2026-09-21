//! Exact gray/mono bitmap extraction from a stock-loaded FreeType glyph slot.

use crate::font::{FontError, RasterizedGlyph};
use freetype::{Face, bitmap::PixelMode};
use solarity_asset::AssetPath;

pub(super) fn glyph_from_slot(
    path: &AssetPath,
    face: &Face<solarity_asset::AssetBytes>,
) -> Result<RasterizedGlyph, FontError> {
    let slot = face.glyph();
    let bitmap = slot.bitmap();
    let width = u32::try_from(bitmap.width()).map_err(|error| bitmap_error(path, error))?;
    let height = u32::try_from(bitmap.rows()).map_err(|error| bitmap_error(path, error))?;
    let coverage = if width == 0 || height == 0 {
        Vec::new()
    } else {
        if bitmap.pitch() < 0 {
            return Err(bitmap_error(path, "negative bitmap pitch"));
        }
        let pitch = usize::try_from(bitmap.pitch()).map_err(|error| bitmap_error(path, error))?;
        let width_usize = usize::try_from(width).map_err(|error| bitmap_error(path, error))?;
        let height_usize = usize::try_from(height).map_err(|error| bitmap_error(path, error))?;
        match bitmap
            .pixel_mode()
            .map_err(|error| bitmap_error(path, error))?
        {
            PixelMode::Gray => {
                copy_gray_bitmap(path, bitmap.buffer(), pitch, width_usize, height_usize)?
            }
            PixelMode::Mono => {
                copy_mono_bitmap(path, bitmap.buffer(), pitch, width_usize, height_usize)?
            }
            mode => return Err(bitmap_error(path, format!("pixel mode {mode:?}"))),
        }
    };

    Ok(RasterizedGlyph {
        width,
        height,
        bearing_x: slot.bitmap_left(),
        bearing_y: slot.bitmap_top(),
        // Stock truncates the signed 26.6 horizontal metric to whole pixels
        // and adds one pixel before retaining it for string layout.
        advance_x_26_6: (i64::from(slot.metrics().horiAdvance) / 64 + 1) * 64,
        coverage: coverage.into(),
    })
}

fn copy_gray_bitmap(
    path: &AssetPath,
    source: &[u8],
    pitch: usize,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, FontError> {
    validate_bitmap_size(path, source, pitch, width, height)?;
    let mut coverage = Vec::with_capacity(width.saturating_mul(height));
    for row in source.chunks_exact(pitch).take(height) {
        coverage.extend_from_slice(&row[..width]);
    }
    Ok(coverage)
}

fn copy_mono_bitmap(
    path: &AssetPath,
    source: &[u8],
    pitch: usize,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, FontError> {
    let packed_width = width.div_ceil(8);
    validate_bitmap_size(path, source, pitch, packed_width, height)?;
    let mut coverage = Vec::with_capacity(width.saturating_mul(height));
    for row in source.chunks_exact(pitch).take(height) {
        for column in 0..width {
            let mask = 0x80_u8 >> (column % 8);
            coverage.push(if row[column / 8] & mask == 0 { 0 } else { 255 });
        }
    }
    Ok(coverage)
}

fn validate_bitmap_size(
    path: &AssetPath,
    source: &[u8],
    pitch: usize,
    row_width: usize,
    height: usize,
) -> Result<(), FontError> {
    let required = pitch
        .checked_mul(height)
        .ok_or_else(|| bitmap_error(path, "bitmap byte size overflow"))?;
    if pitch < row_width || source.len() < required {
        return Err(bitmap_error(
            path,
            format!(
                "{} bytes with pitch {pitch} cannot hold {row_width}x{height} coverage",
                source.len()
            ),
        ));
    }
    Ok(())
}

fn bitmap_error(path: &AssetPath, message: impl ToString) -> FontError {
    FontError::Bitmap {
        path: path.clone(),
        message: message.to_string(),
    }
}
