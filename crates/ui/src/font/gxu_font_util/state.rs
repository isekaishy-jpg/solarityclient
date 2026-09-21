//! Native font faces and exact per-size glyph/metric caches.

use super::{FontRasterization, bitmap::glyph_from_slot};
use crate::font::storage::CacheMap;
use crate::font::{FontError, RasterizedGlyph};
use freetype::face::{KerningMode, LoadFlag};
use freetype::{Face, Library, RenderMode};
use solarity_asset::{AssetPath, AssetStore};

/// A mounted face owns all scalar coverage and native metric results for its sizes.
#[derive(Clone)]
struct FontBytes(std::sync::Arc<FontSource>);
struct FontSource {
    bytes: solarity_asset::AssetBytes,
    _memory: Option<solarity_cpu::ByteReservation>,
}
impl FontBytes {
    fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, FontError> {
        let bytes = store.read(path)?.into_bytes();
        let metadata = size_of::<FontSource>() + 2 * size_of::<usize>();
        let memory = store
            .effective_read_budget()
            .as_ref()
            .map(|budget| {
                budget.storage().reserve(
                    budget.class(),
                    solarity_cpu::CpuStorageKind::Result,
                    metadata,
                )
            })
            .transpose()
            .map_err(solarity_asset::AssetError::from)?;
        Ok(Self(std::sync::Arc::new(FontSource {
            bytes,
            _memory: memory,
        })))
    }
}
impl std::borrow::Borrow<[u8]> for FontBytes {
    fn borrow(&self) -> &[u8] {
        self.0.bytes.as_ref()
    }
}

pub(super) struct CachedFace {
    bytes: FontBytes,
    pub(super) glyphs: CacheMap<(u32, FontRasterization, char), RasterizedGlyph>,
    pub(super) advances: CacheMap<(u32, FontRasterization, char), i64>,
    pub(super) ascenders: CacheMap<u32, i64>,
    pub(super) kerning: CacheMap<(u32, char, char), i64>,
}

/// Plain cache data can travel between workers; no FreeType object is retained here.
#[derive(Default)]
pub(super) struct FontCache {
    pub(super) faces: CacheMap<AssetPath, CachedFace>,
    pub(super) provider: Option<solarity_asset::AssetNamespaceId>,
}

/// Native faces precede the library and are created/retired on the executing thread.
pub(super) struct FontSystemState {
    native_faces: CacheMap<AssetPath, Face<FontBytes>>,
    pub(super) cache: FontCache,
    library: Library,
}

impl FontSystemState {
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
            native_faces: CacheMap::default(),
            cache: FontCache::default(),
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
        self.with_pressure_retry(|state| {
            state.rasterize_inner(store, path, pixel_height, character, rasterization)
        })
    }

    fn rasterize_inner(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        character: char,
        rasterization: FontRasterization,
    ) -> Result<RasterizedGlyph, FontError> {
        if self.cache.provider == Some(store.namespace())
            && let Some(result) = self
                .cache
                .glyph(path, pixel_height, character, rasterization)
                .map(Ok)
        {
            return result;
        }

        let _profile_scope =
            solarity_profiling::detail_profile!("ui.font.gxu_font_util.state.rasterize");
        self.ensure_face(store, path)?;

        let Some(cached) = self.cache.faces.get_mut(path) else {
            return Err(FontError::Face {
                path: path.clone(),
                message: "loaded face was not retained".to_owned(),
            });
        };
        let key = (pixel_height, rasterization, character);
        if let Some(glyph) = cached.glyphs.get(&key) {
            if solarity_profiling::detail_enabled() {
                solarity_profiling::profile_value!("ui.glyph.cache_hit", 1);
            }
            return Ok(glyph.clone());
        }
        let face = &self.native_faces[path];
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

        // GxuFontGlyph in build 12340 loads scalable outlines with 0x208A
        // (NO_HINTING | NO_BITMAP | PEDANTIC | LINEAR_DESIGN), then renders
        // the slot separately. The hinted RENDER shortcut changes both stock
        // advances and coverage.
        let stock_flags = LoadFlag::NO_HINTING
            | LoadFlag::NO_BITMAP
            | LoadFlag::PEDANTIC
            | LoadFlag::LINEAR_DESIGN;
        let (flags, render_mode) = match rasterization {
            FontRasterization::Antialiased => (stock_flags, RenderMode::Normal),
            FontRasterization::Monochrome => (
                stock_flags | LoadFlag::MONOCHROME | LoadFlag::TARGET_MONO,
                RenderMode::Mono,
            ),
        };
        face.load_char(character as usize, flags)
            .map_err(|error| FontError::Glyph {
                path: path.clone(),
                character,
                message: error.to_string(),
            })?;
        face.glyph()
            .render_glyph(render_mode)
            .map_err(|error| FontError::Glyph {
                path: path.clone(),
                character,
                message: error.to_string(),
            })?;

        let budget = store.effective_read_budget();
        let glyph = glyph_from_slot(path, face, budget.as_ref())?;
        if solarity_profiling::detail_enabled() {
            solarity_profiling::profile_value!("ui.glyph.cache_miss", 1);
            solarity_profiling::profile_value!("ui.glyph.rasterized_bytes", glyph.coverage().len());
        }
        cached.glyphs.insert(budget.as_ref(), key, glyph.clone())?;
        Ok(glyph)
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
        let _profile_scope = solarity_profiling::detail_profile!(
            "ui.font.gxu_font_util.state.measure_line_width_26_6"
        );
        self.measure_character_advances_26_6(store, path, pixel_height, text, rasterization)
            .map(|advances| advances.iter().copied().fold(0_i64, i64::saturating_add))
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
    ) -> Result<crate::font::storage::FontBuffer<i64>, FontError> {
        self.with_pressure_retry(|state| {
            state.measure_character_advances_26_6_inner(
                store,
                path,
                pixel_height,
                text,
                rasterization,
            )
        })
    }

    fn measure_character_advances_26_6_inner(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        text: &str,
        rasterization: FontRasterization,
    ) -> Result<crate::font::storage::FontBuffer<i64>, FontError> {
        if self.cache.provider == Some(store.namespace())
            && let Some(result) = self.cache.advances(path, pixel_height, text, rasterization)
        {
            return result;
        }

        let _profile_scope = solarity_profiling::detail_profile!(
            "ui.font.gxu_font_util.state.measure_character_advances_26_6"
        );
        self.ensure_face(store, path)?;
        let Some(cached) = self.cache.faces.get_mut(path) else {
            return Err(FontError::Face {
                path: path.clone(),
                message: "loaded face was not retained".to_owned(),
            });
        };
        let face = &self.native_faces[path];
        face.set_pixel_sizes(0, pixel_height)
            .map_err(|error| FontError::PixelSize {
                path: path.clone(),
                pixel_height,
                message: error.to_string(),
            })?;

        let stock_flags = LoadFlag::NO_HINTING
            | LoadFlag::NO_BITMAP
            | LoadFlag::PEDANTIC
            | LoadFlag::LINEAR_DESIGN;
        let flags = match rasterization {
            FontRasterization::Antialiased => stock_flags,
            FontRasterization::Monochrome => {
                stock_flags | LoadFlag::MONOCHROME | LoadFlag::TARGET_MONO
            }
        };
        let budget = store.effective_read_budget();
        let mut advances = crate::font::storage::FontBuffer::default();
        advances.reserve(budget.as_ref(), text.chars().count())?;
        for character in text.chars() {
            let key = (pixel_height, rasterization, character);
            if let Some(&advance) = cached.advances.get(&key) {
                advances.push(budget.as_ref(), advance)?;
                continue;
            }
            let Some(index) = face.get_char_index(character as usize) else {
                return Err(FontError::Glyph {
                    path: path.clone(),
                    character,
                    message: "font face does not contain the requested character".to_owned(),
                });
            };
            face.load_glyph(index, flags)
                .map_err(|error| FontError::Glyph {
                    path: path.clone(),
                    character,
                    message: error.to_string(),
                })?;
            let advance = i64::from(face.glyph().metrics().horiAdvance) / 64 + 1;
            let advance = advance.saturating_mul(64);
            cached
                .advances
                .insert(store.effective_read_budget().as_ref(), key, advance)?;
            advances.push(budget.as_ref(), advance)?;
        }
        Ok(advances)
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
        self.with_pressure_retry(|state| state.ascender_26_6_inner(store, path, pixel_height))
    }

    fn ascender_26_6_inner(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
    ) -> Result<i64, FontError> {
        if self.cache.provider == Some(store.namespace())
            && let Some(result) = self
                .cache
                .faces
                .get(path)
                .and_then(|face| face.ascenders.get(&pixel_height))
                .copied()
                .map(Ok)
        {
            return result;
        }

        self.ensure_face(store, path)?;
        let cached = self
            .cache
            .faces
            .get_mut(path)
            .ok_or_else(|| FontError::Face {
                path: path.clone(),
                message: "loaded face was not retained".to_owned(),
            })?;
        if let Some(&ascender) = cached.ascenders.get(&pixel_height) {
            return Ok(ascender);
        }
        let face = &self.native_faces[path];
        face.set_pixel_sizes(0, pixel_height)
            .map_err(|error| FontError::PixelSize {
                path: path.clone(),
                pixel_height,
                message: error.to_string(),
            })?;
        let ascender = crate::font::pixel_size::ascender_pixels(
            face.ascender(),
            face.descender(),
            pixel_height,
        )
        .map(|ascender| ascender * 64)
        .ok_or_else(|| FontError::Face {
            path: path.clone(),
            message: "face has no vertical metric span".to_owned(),
        })?;
        cached.ascenders.insert(
            store.effective_read_budget().as_ref(),
            pixel_height,
            ascender,
        )?;
        Ok(ascender)
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
        self.with_pressure_retry(|state| {
            state.kerning_x_26_6_inner(store, path, pixel_height, left, right)
        })
    }

    fn kerning_x_26_6_inner(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        left: char,
        right: char,
    ) -> Result<i64, FontError> {
        if self.cache.provider == Some(store.namespace())
            && let Some(result) = self
                .cache
                .faces
                .get(path)
                .and_then(|face| face.kerning.get(&(pixel_height, left, right)))
                .copied()
                .map(Ok)
        {
            return result;
        }

        self.ensure_face(store, path)?;
        let cached = self
            .cache
            .faces
            .get_mut(path)
            .ok_or_else(|| FontError::Face {
                path: path.clone(),
                message: "loaded face was not retained".to_owned(),
            })?;
        let key = (pixel_height, left, right);
        if let Some(&kerning) = cached.kerning.get(&key) {
            return Ok(kerning);
        }
        let face = &self.native_faces[path];
        face.set_pixel_sizes(0, pixel_height)
            .map_err(|error| FontError::PixelSize {
                path: path.clone(),
                pixel_height,
                message: error.to_string(),
            })?;
        let left_index = face
            .get_char_index(left as usize)
            .ok_or_else(|| FontError::Glyph {
                path: path.clone(),
                character: left,
                message: "font face does not contain the requested character".to_owned(),
            })?;
        let right_index = face
            .get_char_index(right as usize)
            .ok_or_else(|| FontError::Glyph {
                path: path.clone(),
                character: right,
                message: "font face does not contain the requested character".to_owned(),
            })?;
        let kerning = face
            .get_kerning(left_index, right_index, KerningMode::KerningDefault)
            .map(|kerning| i64::from(kerning.x))
            .map_err(|error| FontError::Glyph {
                path: path.clone(),
                character: right,
                message: format!("failed to read kerning: {error}"),
            })?;
        cached
            .kerning
            .insert(store.effective_read_budget().as_ref(), key, kerning)?;
        Ok(kerning)
    }

    /// Returns the number of distinct archive-backed faces retained in memory.
    #[must_use]
    pub fn loaded_face_count(&self) -> usize {
        self.cache.faces.len()
    }

    /// Reports retained common coverage without counting shared clones twice.
    pub(super) fn coverage_bytes(&self) -> usize {
        self.cache
            .faces
            .values()
            .flat_map(|face| face.glyphs.values())
            .map(|glyph| glyph.coverage().len())
            .sum()
    }

    pub(super) fn glyph_count(&self) -> usize {
        self.cache
            .faces
            .values()
            .map(|face| face.glyphs.len())
            .sum()
    }

    fn with_pressure_retry<T>(
        &mut self,
        mut operation: impl FnMut(&mut Self) -> Result<T, FontError>,
    ) -> Result<T, FontError> {
        match operation(self) {
            Err(FontError::Asset(solarity_asset::AssetError::SourceStorage(
                solarity_cpu::CpuError::StorageAtCapacity { .. },
            ))) => {
                self.trim_unused();
                operation(self)
            }
            result => result,
        }
    }

    pub(super) fn trim_unused(&mut self) {
        // Native handles pin encoded sources and must retire before source trimming.
        self.native_faces.clear();
        self.cache.trim_unused();
    }

    fn ensure_face(&mut self, store: &mut AssetStore, path: &AssetPath) -> Result<(), FontError> {
        let _profile_scope =
            solarity_profiling::detail_profile!("ui.font.gxu_font_util.state.ensure_face");
        if self.cache.provider != Some(store.namespace()) {
            self.native_faces.clear();
            self.cache.faces.clear();
            self.cache.provider = Some(store.namespace());
        }
        if self.native_faces.contains_key(path) {
            return Ok(());
        }
        let bytes = match self.cache.faces.get(path) {
            Some(cached) => cached.bytes.clone(),
            None => FontBytes::load(store, path)?,
        };
        let face = self
            .library
            .new_memory_face2(bytes.clone(), 0)
            .map_err(|error| FontError::Face {
                path: path.clone(),
                message: error.to_string(),
            })?;
        let budget = store.effective_read_budget();
        self.native_faces
            .insert(budget.as_ref(), path.clone(), face)?;
        if !self.cache.faces.contains_key(path)
            && let Err(error) = self.cache.faces.insert(
                budget.as_ref(),
                path.clone(),
                CachedFace {
                    bytes,
                    glyphs: CacheMap::default(),
                    advances: CacheMap::default(),
                    ascenders: CacheMap::default(),
                    kerning: CacheMap::default(),
                },
            )
        {
            self.native_faces.clear();
            return Err(error);
        }
        Ok(())
    }
}

impl FontCache {
    pub(super) fn trim_unused(&mut self) {
        for face in self.faces.values_mut() {
            face.glyphs.retain(|_, glyph| glyph.coverage.is_pinned());
            face.advances.clear();
            face.ascenders.clear();
            face.kerning.clear();
        }
        self.faces.retain(|_, face| !face.glyphs.is_empty());
    }
}
