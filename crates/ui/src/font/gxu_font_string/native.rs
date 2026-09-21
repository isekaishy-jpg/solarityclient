//! Immutable native text products cross worker boundaries without FreeType ownership.

use super::*;

/// Fixed native-overlay glyph coverage and metrics. The creating worker releases
/// its font library and faces before publishing this immutable render input.
#[derive(Debug, PartialEq)]
pub struct UiNativeTextAtlas {
    identity: u64,
    page: UiGlyphAtlasPage,
    glyphs: CacheMap<GlyphKey, RasterizedGlyph>,
    placements: CacheMap<GlyphKey, AtlasPlacement>,
    metrics: CacheMap<LineFontKey, FontMetrics>,
    native_font: LineFontKey,
}

impl UiNativeTextAtlas {
    /// Returns the immutable atlas identity used by the renderer.
    #[must_use]
    pub const fn identity(&self) -> u64 {
        self.identity
    }
    /// Returns the retained coverage texture extent.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.page.extent()
    }
    /// Returns tightly packed linear RGBA8 coverage pixels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        self.page.rgba8()
    }

    /// Rasterizes one fixed native-overlay character repertoire.
    ///
    /// The resulting atlas can rebuild small text meshes without rerasterizing
    /// archive glyphs or changing its sampled-image identity.
    pub fn prepare(
        style: &UiNativeTextStyle,
        characters: &str,
        assets: &mut AssetStore,
        display_height: u32,
    ) -> Result<Self, FontError> {
        let pixels_per_ui_unit = f64::from(display_height) / 768.0;
        let font = native_font_key(style, pixels_per_ui_unit)?;
        let mut keys = characters
            .chars()
            .filter(|character| !character.is_control())
            .map(|character| GlyphKey::new(&font, character))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        keys.sort_by_key(|key| key.character);
        let mut system = FontSystem::new()?;
        let budget = assets.effective_read_budget();
        let mut glyphs = CacheMap::default();
        glyphs.reserve(budget.as_ref(), keys.len())?;
        for key in &keys {
            let glyph = system.rasterize(
                assets,
                &key.face,
                key.pixel_height,
                key.character,
                key.rasterization,
            )?;
            glyphs.insert(budget.as_ref(), key.clone(), glyph)?;
        }
        let (extent, placements) = pack(&keys, &glyphs, budget.as_ref())?;
        let rgba8 = compose_atlas(
            extent,
            &keys,
            &glyphs,
            &placements,
            assets.effective_read_budget().as_ref(),
        )?;
        let ascender_26_6 = system.ascender_26_6(assets, &font.face, font.pixel_height)?;
        let mut metrics = CacheMap::default();
        metrics.insert(budget.as_ref(), font.clone(), FontMetrics { ascender_26_6 })?;
        let identity = next_identity();
        let next_row = placements
            .iter()
            .map(|(key, place)| place.y + glyphs[key].height() + GLYPH_PADDING * 2)
            .max()
            .unwrap_or(GLYPH_PADDING);
        Ok(Self {
            identity,
            page: UiGlyphAtlasPage::packed(identity, extent, rgba8, next_row),
            glyphs,
            placements,
            metrics,
            native_font: font,
        })
    }

    /// Builds one top-left-anchored native text mesh from retained glyphs.
    pub fn native_text_mesh(
        &self,
        text: &str,
        style: &UiNativeTextStyle,
        logical_extent: [f32; 2],
        top_left: [f32; 2],
        region_height: f32,
        display_height: u32,
    ) -> Result<UiMeshPlan, FontError> {
        let pixels_per_ui_unit = f64::from(display_height) / 768.0;
        let font = native_font_key(style, pixels_per_ui_unit)?;
        if self.native_font != font {
            return Err(FontError::Presentation {
                message: "native text style does not match its retained atlas".to_owned(),
            });
        }
        let metrics = self
            .metrics
            .get(&font)
            .ok_or_else(|| FontError::Presentation {
                message: "native text atlas has no retained font metrics".to_owned(),
            })?;
        let line_height = style.height;
        let line_top = logical_extent[1] - top_left[1] - (region_height - line_height) * 0.5;
        let ascender = metrics.ascender_26_6 as f64 / 64.0 / pixels_per_ui_unit;
        let baseline = f64::from(line_top) - ascender;
        let mut pen_x = f64::from(top_left[0]);
        let mut glyph_quads = Vec::new();
        for character in text.chars().filter(|character| !character.is_control()) {
            let key = GlyphKey::new(&font, character);
            let glyph = self
                .glyphs
                .get(&key)
                .ok_or_else(|| FontError::Presentation {
                    message: format!("native text uses unavailable glyph {character:?}"),
                })?;
            if glyph.width() > 0 && glyph.height() > 0 {
                let placement =
                    self.placements
                        .get(&key)
                        .ok_or_else(|| FontError::Presentation {
                            message: format!("native glyph {character:?} has no atlas placement"),
                        })?;
                let left = pen_x + f64::from(glyph.bearing_x()) / pixels_per_ui_unit;
                let top = baseline + f64::from(glyph.bearing_y()) / pixels_per_ui_unit;
                let right = left + f64::from(glyph.width()) / pixels_per_ui_unit;
                let bottom = top - f64::from(glyph.height()) / pixels_per_ui_unit;
                let u0 = placement.x as f32 / placement.extent.0 as f32;
                let v0 = placement.y as f32 / placement.extent.1 as f32;
                let u1 = (placement.x + glyph.width()) as f32 / placement.extent.0 as f32;
                let v1 = (placement.y + glyph.height()) as f32 / placement.extent.1 as f32;
                glyph_quads.push((
                    [left as f32, bottom as f32, right as f32, top as f32],
                    [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                ));
            }
            pen_x += glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit;
        }
        let mut quads = Vec::with_capacity(glyph_quads.len() * 10);
        let outline = style.outline_width.max(0.0);
        if outline > 0.0 {
            for (bounds, coordinates) in &glyph_quads {
                for offset in [
                    [-outline, -outline],
                    [0.0, -outline],
                    [outline, -outline],
                    [-outline, 0.0],
                    [outline, 0.0],
                    [-outline, outline],
                    [0.0, outline],
                    [outline, outline],
                ] {
                    quads.push(native_glyph_quad(
                        self.identity,
                        offset_bounds(*bounds, offset),
                        *coordinates,
                        style.outline_color,
                    ));
                }
            }
        }
        for (bounds, coordinates) in &glyph_quads {
            quads.push(native_glyph_quad(
                self.identity,
                offset_bounds(*bounds, [style.shadow_offset[0], -style.shadow_offset[1]]),
                *coordinates,
                style.shadow_color,
            ));
        }
        for (bounds, coordinates) in glyph_quads {
            quads.push(native_glyph_quad(
                self.identity,
                bounds,
                coordinates,
                style.color,
            ));
        }
        UiMeshPlan::prepare(logical_extent, quads.into_iter()).map_err(|error| {
            FontError::Presentation {
                message: error.to_string(),
            }
        })
    }

    /// Measures one line using the exact advances retained by a native atlas.
    ///
    /// Native loading and diagnostic surfaces use this to center text without
    /// substituting operating-system font metrics.
    pub fn native_text_width(
        &self,
        text: &str,
        style: &UiNativeTextStyle,
        display_height: u32,
    ) -> Result<f32, FontError> {
        let pixels_per_ui_unit = f64::from(display_height) / 768.0;
        let font = native_font_key(style, pixels_per_ui_unit)?;
        if self.native_font != font {
            return Err(FontError::Presentation {
                message: "native text style does not match its retained atlas".to_owned(),
            });
        }
        text.chars()
            .filter(|character| !character.is_control())
            .try_fold(0.0_f64, |width, character| {
                let key = GlyphKey::new(&font, character);
                let glyph = self
                    .glyphs
                    .get(&key)
                    .ok_or_else(|| FontError::Presentation {
                        message: format!("native text uses unavailable glyph {character:?}"),
                    })?;
                Ok(width + glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit)
            })
            .map(|width| width as f32)
    }
}
