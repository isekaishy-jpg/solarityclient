//! Archive-backed FreeType ownership and glyph rasterization.

use std::collections::HashMap;

use freetype::bitmap::PixelMode;
use freetype::face::LoadFlag;
use freetype::{Face, Library};
use solarity_asset::{AssetPath, AssetStore};

use crate::font::{FontError, RasterizedGlyph};

/// Stock font smoothing mode selected by a font object's `monochrome` flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontRasterization {
    /// Grayscale antialiased coverage.
    Antialiased,
    /// One-bit coverage expanded to bytes for atlas storage.
    Monochrome,
}

/// The UI-thread owner of FreeType and lazily loaded stock font faces.
///
/// Faces retain their archive bytes internally. The face map is declared
/// before the library so Rust drops every face before the FreeType library it
/// references.
pub struct FontSystem {
    faces: HashMap<AssetPath, Face>,
    library: Library,
}

impl FontSystem {
    /// Initializes the process-local FreeType owner.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Library`] when FreeType initialization fails.
    pub fn new() -> Result<Self, FontError> {
        let library = Library::init().map_err(|error| FontError::Library {
            message: error.to_string(),
        })?;
        Ok(Self {
            faces: HashMap::new(),
            library,
        })
    }

    /// Loads a stock font face on demand and rasterizes one Unicode character.
    ///
    /// Each distinct archive path is read once and retained by FreeType. Pixel
    /// height remains part of the glyph request because many stock font
    /// objects share a face at different sizes.
    ///
    /// # Errors
    ///
    /// Returns an asset, face, size, glyph, or bitmap error. The method never
    /// substitutes an operating-system font when the selected stock face lacks
    /// a glyph or cannot be decoded.
    pub fn rasterize(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        character: char,
        rasterization: FontRasterization,
    ) -> Result<RasterizedGlyph, FontError> {
        if !self.faces.contains_key(path) {
            let bytes = store.read(path)?.into_bytes();
            let face = self
                .library
                .new_memory_face(bytes, 0)
                .map_err(|error| FontError::Face {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
            self.faces.insert(path.clone(), face);
        }

        let Some(face) = self.faces.get(path) else {
            return Err(FontError::Face {
                path: path.clone(),
                message: "loaded face was not retained".to_owned(),
            });
        };
        face.set_pixel_sizes(0, pixel_height)
            .map_err(|error| FontError::PixelSize {
                path: path.clone(),
                pixel_height,
                message: error.to_string(),
            })?;
        if face.get_char_index(character as usize).is_none() {
            return Err(FontError::Glyph {
                path: path.clone(),
                character,
                message: "font face does not contain the requested character".to_owned(),
            });
        }

        let flags = match rasterization {
            FontRasterization::Antialiased => LoadFlag::RENDER | LoadFlag::TARGET_NORMAL,
            FontRasterization::Monochrome => {
                LoadFlag::RENDER | LoadFlag::MONOCHROME | LoadFlag::TARGET_MONO
            }
        };
        face.load_char(character as usize, flags)
            .map_err(|error| FontError::Glyph {
                path: path.clone(),
                character,
                message: error.to_string(),
            })?;

        glyph_from_slot(path, face)
    }

    /// Returns the number of distinct archive-backed faces retained in memory.
    #[must_use]
    pub fn loaded_face_count(&self) -> usize {
        self.faces.len()
    }
}

fn glyph_from_slot(path: &AssetPath, face: &Face) -> Result<RasterizedGlyph, FontError> {
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
        advance_x_26_6: i64::from(slot.advance().x),
        coverage,
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
