//! Coverage admission is independent of text layout and UI topology publication.

use super::{
    FontCatalog, FontError, FontMetrics, GlyphKey, LineFontKey, UiGlyphAtlasPage, UiGlyphAtlasPlan,
    UiRegionGeometryPlan, UiRuntimeObjectPlan, UiSimpleHtmlPlan, coverage, font_definition,
    font_key, layout_quads, presented_characters, runtime_font_key,
};
use solarity_asset::AssetStore;

impl UiGlyphAtlasPlan {
    /// Stable image pages, in allocation order.
    pub fn pages(&self) -> &[UiGlyphAtlasPage] {
        &self.pages
    }

    /// Changes only when new glyph coverage is admitted.
    pub const fn coverage_revision(&self) -> u64 {
        self.coverage_revision
    }

    /// Resolves a quad's page to its renderer resource identity.
    pub fn page_identity(&self, page: usize) -> u64 {
        self.pages[page].identity()
    }

    /// Admits coverage only for named owners. Existing placements and unrelated
    /// layout remain untouched, even when another sampled-image page is needed.
    pub(crate) fn ensure_live_text_objects(
        &mut self,
        live: &UiRuntimeObjectPlan,
        assets: &mut AssetStore,
        system: &mut crate::FontSystem,
        logical_height: u32,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<(), FontError> {
        let scale = f64::from(logical_height) / 768.0;
        for index in indices {
            let Some(text) = live
                .objects()
                .get(index)
                .and_then(|object| object.text.as_ref())
            else {
                continue;
            };
            let font = runtime_font_key(text, scale)?;
            self.ensure_font(assets, system, &font)?;
            for character in presented_characters(text)
                .into_iter()
                .map(|value| value.character)
                .filter(|value| !value.is_control())
            {
                self.ensure_glyph(assets, system, GlyphKey::new(&font, character))?;
            }
        }
        Ok(())
    }

    /// A document/topology change relays out current owners while keeping the
    /// common coverage bank. No previous glyph image generation is recreated.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rebuild_live_ui(
        &mut self,
        html: &UiSimpleHtmlPlan,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        system: &mut crate::FontSystem,
        logical_height: u32,
    ) -> Result<(), FontError> {
        self.ensure_live_text_objects(
            live,
            assets,
            system,
            logical_height,
            0..live.objects().len(),
        )?;
        let scale = f64::from(logical_height) / 768.0;
        for index in 0..geometry.region_count() {
            let Some(node) = html.node(index) else {
                continue;
            };
            for line in node.lines() {
                let font = font_key(font_definition(fonts, line.font_object())?, scale)?;
                self.ensure_font(assets, system, &font)?;
                for character in line.text().chars() {
                    self.ensure_glyph(assets, system, GlyphKey::new(&font, character))?;
                }
            }
        }
        let extent = self.extent();
        let layout = layout_quads(
            html,
            Some(live),
            geometry,
            fonts,
            assets,
            scale,
            system,
            &self.glyphs,
            &self.placements,
            extent,
        )?;
        self.html_quads = layout.quads;
        self.html_runs = layout.runs;
        self.refresh_live_text(live, geometry, logical_height)
    }

    /// Font metrics and the stock western repertoire are admitted once per key.
    fn ensure_font(
        &mut self,
        assets: &mut AssetStore,
        system: &mut crate::FontSystem,
        font: &LineFontKey,
    ) -> Result<(), FontError> {
        if self.metrics.contains_key(font) {
            return Ok(());
        }
        let ascender_26_6 = system.ascender_26_6(assets, &font.face, font.pixel_height)?;
        let characters = (0x20..=0xff).filter_map(char::from_u32).collect::<Vec<_>>();
        let glyphs = system.rasterize_batch(
            assets,
            characters
                .iter()
                .map(|&character| {
                    crate::FontGlyphRequest::new(
                        font.face.clone(),
                        font.pixel_height,
                        character,
                        font.rasterization,
                    )
                })
                .collect(),
        )?;
        for (character, glyph) in characters.into_iter().zip(glyphs) {
            match glyph.and_then(|glyph| self.install_glyph(GlyphKey::new(font, character), glyph))
            {
                // As in initial packing, optional stock prewarm coverage may be
                // absent. Required text below still reports a missing glyph.
                Err(FontError::Glyph { .. }) => (),
                result => result?,
            }
        }
        self.metrics
            .insert(font.clone(), FontMetrics { ascender_26_6 });
        Ok(())
    }

    /// Publishes a newly cached bitmap into unused page texels exactly once.
    fn ensure_glyph(
        &mut self,
        assets: &mut AssetStore,
        system: &mut crate::FontSystem,
        key: GlyphKey,
    ) -> Result<(), FontError> {
        if self.glyphs.contains_key(&key) {
            return Ok(());
        }
        let glyph = system.rasterize(
            assets,
            &key.face,
            key.pixel_height,
            key.character,
            key.rasterization,
        )?;
        self.install_glyph(key, glyph)
    }

    fn install_glyph(
        &mut self,
        key: GlyphKey,
        glyph: crate::RasterizedGlyph,
    ) -> Result<(), FontError> {
        if self.glyphs.contains_key(&key) {
            return Ok(());
        }
        let placement = coverage::insert(&mut self.pages, &glyph)?;
        self.placements.insert(key.clone(), placement);
        self.glyphs.insert(key, glyph);
        self.coverage_revision += 1;
        Ok(())
    }
}
