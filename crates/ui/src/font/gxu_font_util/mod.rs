//! Shared UI-thread font ownership and persistent archive-backed glyph coverage.

mod bitmap;
mod state;

use crate::font::{FontError, RasterizedGlyph};
use solarity_asset::{AssetPath, AssetStore};
use state::FontSystemState;
use std::cell::RefCell;
use std::rc::Rc;

/// Stock font smoothing mode selected by a font object's `monochrome` flag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FontRasterization {
    /// Grayscale antialiased coverage.
    Antialiased,
    /// One-bit coverage expanded to bytes for atlas storage.
    Monochrome,
}

/// Clones share one mounted-provider cache; layout and glyph consumers use the
/// same faces, coverage and metrics. No global cache or cross-thread lock exists.
#[derive(Clone)]
pub struct FontSystem {
    state: Rc<RefCell<FontSystemState>>,
}

impl std::fmt::Debug for FontSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSystem")
            .field("faces", &self.loaded_face_count())
            .field("glyphs", &self.cached_glyph_count())
            .finish()
    }
}
impl PartialEq for FontSystem {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

impl FontSystem {
    /// Initializes a scoped native owner; clones retain its useful common data.
    /// # Errors
    /// Returns `FontError::Library` when FreeType initialization fails.
    pub fn new() -> Result<Self, FontError> {
        Ok(Self {
            state: Rc::new(RefCell::new(FontSystemState::new()?)),
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
        self.state
            .borrow_mut()
            .rasterize(store, path, pixel_height, character, rasterization)
    }

    /// Measures one line using build-12340's unfitted whole-pixel advances.
    ///
    /// Additional XML spacing and shadow extent remain string-object concerns,
    /// so callers apply those after converting the returned 26.6-pixel value.
    ///
    /// # Errors
    ///
    /// Returns an asset, face, size, glyph, or kerning error without replacing
    /// a missing stock glyph or font face.
    pub fn measure_line_width_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        text: &str,
        rasterization: FontRasterization,
    ) -> Result<i64, FontError> {
        self.state.borrow_mut().measure_line_width_26_6(
            store,
            path,
            pixel_height,
            text,
            rasterization,
        )
    }

    /// Measures each scalar after selecting one stock face and size.
    ///
    /// FontString wrapping consumes the same unfitted advances as ordinary
    /// line measurement without reopening or resizing the face per scalar.
    pub(crate) fn measure_character_advances_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        text: &str,
        rasterization: FontRasterization,
    ) -> Result<Vec<i64>, FontError> {
        self.state.borrow_mut().measure_character_advances_26_6(
            store,
            path,
            pixel_height,
            text,
            rasterization,
        )
    }

    /// Returns the build-12340 face ascender for one pixel height.
    ///
    /// # Errors
    ///
    /// Returns the same archive, face, or size failures as glyph loading and
    /// rejects a face whose ascender and descender define no vertical span.
    pub fn ascender_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
    ) -> Result<i64, FontError> {
        self.state
            .borrow_mut()
            .ascender_26_6(store, path, pixel_height)
    }

    /// Returns hinted horizontal kerning for one adjacent character pair.
    ///
    /// # Errors
    ///
    /// Returns an archive, face, size, glyph, or FreeType kerning failure.
    pub fn kerning_x_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        left: char,
        right: char,
    ) -> Result<i64, FontError> {
        self.state
            .borrow_mut()
            .kerning_x_26_6(store, path, pixel_height, left, right)
    }

    /// Returns the number of distinct archive-backed faces retained in memory.
    #[must_use]
    pub fn loaded_face_count(&self) -> usize {
        self.state.borrow().loaded_face_count()
    }

    /// Number of retained font/size/mode/scalar coverage entries.
    pub fn cached_glyph_count(&self) -> usize {
        self.state.borrow().glyph_count()
    }

    /// Common coverage bytes, excluding atlas copies and shared glyph references.
    pub fn cached_coverage_bytes(&self) -> usize {
        self.state.borrow().coverage_bytes()
    }
}
