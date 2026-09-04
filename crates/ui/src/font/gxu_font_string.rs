//! Deduplicated glyph-atlas and quad generation for retained font strings.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::{AssetPath, AssetStore};
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiTextureAddressMode,
    UiTextureResidency,
};

use super::EditBoxTextLayout;
use crate::script::{UiRuntimeObjectPlan, UiRuntimeText, UiRuntimeTextColorChange};
use crate::widget::nearest_owning_scroll_frame;
use crate::{
    FontCatalog, FontError, FontRasterization, FontSystem, RasterizedGlyph, UiObjectKind,
    UiObjectRole, UiPresentationPacketKey, UiRegionGeometryPlan, UiScrollFramePlan,
    UiSimpleHtmlAlignment, UiSimpleHtmlPlan,
};

const ATLAS_ROW_WIDTH: u32 = 512;
const GLYPH_PADDING: u32 = 1;

/// One positioned glyph sampling the current immutable coverage atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphQuad {
    packet_key: Option<UiPresentationPacketKey>,
    object_index: usize,
    clip_object: Option<usize>,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    color: [f32; 4],
    caret: bool,
}

impl UiGlyphQuad {
    pub(crate) const fn packet_key(&self) -> Option<UiPresentationPacketKey> {
        self.packet_key
    }

    pub(crate) const fn clip_object(&self) -> Option<usize> {
        self.clip_object
    }

    /// Returns the owning static UI object index.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns left, bottom, right, and top edges in presentation units.
    #[must_use]
    pub const fn bounds(&self) -> [f32; 4] {
        self.bounds
    }

    /// Returns upper-left, lower-left, upper-right, and lower-right UV pairs.
    #[must_use]
    pub const fn texture_coordinates(&self) -> [[f32; 2]; 4] {
        self.texture_coordinates
    }

    /// Returns the authored stock font color before inherited region opacity.
    #[must_use]
    pub const fn color(&self) -> [f32; 4] {
        self.color
    }

    pub(crate) const fn is_caret(&self) -> bool {
        self.caret
    }
}

/// One immutable coverage texture and every glyph quad that samples it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphAtlasPlan {
    identity: u64,
    extent: (u32, u32),
    rgba8: Vec<u8>,
    html_quads: Vec<LocalGlyphQuad>,
    html_runs: Vec<LocalGlyphRun>,
    live_quads: Vec<LocalGlyphQuad>,
    edit_box_layouts: Vec<Option<EditBoxTextLayout>>,
    glyphs: HashMap<GlyphKey, RasterizedGlyph>,
    placements: HashMap<GlyphKey, AtlasPlacement>,
    metrics: HashMap<LineFontKey, FontMetrics>,
    native_font: Option<LineFontKey>,
}

/// One archive-backed font and material contract for native text overlays.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNativeTextStyle {
    face: AssetPath,
    height: f32,
    rasterization: FontRasterization,
    color: [f32; 4],
    shadow_color: [f32; 4],
    shadow_offset: [f32; 2],
    outline_color: [f32; 4],
    outline_width: f32,
}

impl UiNativeTextStyle {
    /// Captures one explicit native text style without operating-system fonts.
    #[must_use]
    pub const fn new(face: AssetPath, height: f32, rasterization: FontRasterization) -> Self {
        Self {
            face,
            height,
            rasterization,
            color: [1.0; 4],
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            outline_color: [0.0; 4],
            outline_width: 0.0,
        }
    }

    /// Selects the primary glyph color.
    #[must_use]
    pub const fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// Adds one colorized shadow at the requested top-left-space offset.
    #[must_use]
    pub const fn with_shadow(mut self, color: [f32; 4], offset: [f32; 2]) -> Self {
        self.shadow_color = color;
        self.shadow_offset = offset;
        self
    }

    /// Adds an eight-sample outline around each rasterized glyph.
    #[must_use]
    pub const fn with_outline(mut self, color: [f32; 4], width: f32) -> Self {
        self.outline_color = color;
        self.outline_width = width;
        self
    }
}

impl UiGlyphAtlasPlan {
    /// Rasterizes all static `SimpleHTML` text into one atlas generation.
    ///
    /// Glyph identity includes the exact archive face, hinted pixel height,
    /// rasterization mode, and Unicode scalar. Repeated legal-copy characters
    /// therefore share coverage while retaining per-line kerning and color.
    ///
    /// # Errors
    ///
    /// Returns the first archive-backed font failure or an atlas-size overflow.
    pub fn from_simple_html(
        html: &UiSimpleHtmlPlan,
        geometry: &UiRegionGeometryPlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<Self, FontError> {
        Self::build(html, None, geometry, fonts, assets, logical_height)
    }

    /// Rasterizes one fixed native-overlay character repertoire.
    ///
    /// The resulting atlas can rebuild small text meshes without rerasterizing
    /// archive glyphs or changing its sampled-image identity.
    pub fn from_native_text(
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
        let glyphs = keys
            .iter()
            .map(|key| {
                system
                    .rasterize(
                        assets,
                        &key.face,
                        key.pixel_height,
                        key.character,
                        key.rasterization,
                    )
                    .map(|glyph| (key.clone(), glyph))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let (extent, placements) = pack(&keys, &glyphs)?;
        let rgba8 = compose_atlas(extent, &keys, &glyphs, &placements)?;
        let ascender_26_6 = system.ascender_26_6(assets, &font.face, font.pixel_height)?;
        let metrics = HashMap::from([(font.clone(), FontMetrics { ascender_26_6 })]);
        Ok(Self {
            identity: next_identity(),
            extent,
            rgba8,
            html_quads: Vec::new(),
            html_runs: Vec::new(),
            live_quads: Vec::new(),
            edit_box_layouts: Vec::new(),
            glyphs,
            placements,
            metrics,
            native_font: Some(font),
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
        if self.native_font.as_ref() != Some(&font) {
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
                let u0 = placement.x as f32 / self.extent.0 as f32;
                let v0 = placement.y as f32 / self.extent.1 as f32;
                let u1 = (placement.x + glyph.width()) as f32 / self.extent.0 as f32;
                let v1 = (placement.y + glyph.height()) as f32 / self.extent.1 as f32;
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
        if self.native_font.as_ref() != Some(&font) {
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

    /// Rasterizes static documents and the live ordinary text-object arena.
    pub(crate) fn from_live_ui(
        html: &UiSimpleHtmlPlan,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<Self, FontError> {
        Self::build(html, Some(live), geometry, fonts, assets, logical_height)
    }

    fn build(
        html: &UiSimpleHtmlPlan,
        live: Option<&UiRuntimeObjectPlan>,
        geometry: &UiRegionGeometryPlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<Self, FontError> {
        let pixels_per_ui_unit = f64::from(logical_height) / 768.0;
        let mut requested = HashSet::new();
        for object_index in 0..geometry.region_count() {
            let Some(node) = html.node(object_index) else {
                continue;
            };
            for line in node.lines() {
                let definition = font_definition(fonts, line.font_object())?;
                let key = font_key(definition, pixels_per_ui_unit)?;
                for character in line.text().chars() {
                    requested.insert(GlyphKey {
                        face: key.face.clone(),
                        pixel_height: key.pixel_height,
                        rasterization: key.rasterization,
                        character,
                    });
                }
            }
        }
        let mut required = requested.clone();
        if let Some(live) = live {
            request_live_glyphs(live, pixels_per_ui_unit, &mut requested, &mut required)?;
        }

        let mut system = FontSystem::new()?;
        let mut requested_keys = requested.into_iter().collect::<Vec<_>>();
        requested_keys.sort_by(|left, right| {
            left.face
                .as_str()
                .cmp(right.face.as_str())
                .then(left.pixel_height.cmp(&right.pixel_height))
                .then(raster_rank(left.rasterization).cmp(&raster_rank(right.rasterization)))
                .then(left.character.cmp(&right.character))
        });
        let mut keys = Vec::with_capacity(requested_keys.len());
        let mut glyphs = HashMap::with_capacity(requested_keys.len());
        for key in requested_keys {
            let glyph = match system.rasterize(
                assets,
                &key.face,
                key.pixel_height,
                key.character,
                key.rasterization,
            ) {
                Ok(glyph) => glyph,
                Err(FontError::Glyph { .. }) if !required.contains(&key) => continue,
                Err(error) => return Err(error),
            };
            glyphs.insert(key.clone(), glyph);
            keys.push(key);
        }
        let (extent, placements) = pack(&keys, &glyphs)?;
        let rgba8 = compose_atlas(extent, &keys, &glyphs, &placements)?;
        let html_layout = layout_quads(
            html,
            live,
            geometry,
            fonts,
            assets,
            pixels_per_ui_unit,
            &mut system,
            &glyphs,
            &placements,
            extent,
        )?;
        let mut metrics = HashMap::new();
        for key in keys.iter().map(GlyphKey::font).collect::<HashSet<_>>() {
            let ascender_26_6 = system.ascender_26_6(assets, &key.face, key.pixel_height)?;
            metrics.insert(key, FontMetrics { ascender_26_6 });
        }
        let live_layout = live.map_or(Ok(LiveTextLayout::default()), |live| {
            layout_live_quads(
                live,
                geometry,
                pixels_per_ui_unit,
                &glyphs,
                &placements,
                &metrics,
                extent,
            )
        })?;
        Ok(Self {
            identity: next_identity(),
            extent,
            rgba8,
            html_quads: html_layout.quads,
            html_runs: html_layout.runs,
            live_quads: live_layout.quads,
            edit_box_layouts: live_layout.edit_boxes,
            glyphs,
            placements,
            metrics,
            native_font: None,
        })
    }

    /// Returns whether the retained baseline atlas covers current live text.
    #[must_use]
    pub(crate) fn supports_live_text(
        &self,
        live: &UiRuntimeObjectPlan,
        logical_height: u32,
    ) -> bool {
        let pixels_per_ui_unit = f64::from(logical_height) / 768.0;
        live.objects().iter().all(|object| {
            object.text.as_ref().is_none_or(|text| {
                runtime_font_key(text, pixels_per_ui_unit).is_ok_and(|font| {
                    presented_characters(text)
                        .into_iter()
                        .filter(|presented| !presented.character.is_control())
                        .all(|presented| {
                            self.glyphs
                                .contains_key(&GlyphKey::new(&font, presented.character))
                        })
                })
            })
        })
    }

    /// Repositions ordinary live text without rerasterizing the retained atlas.
    pub(crate) fn refresh_live_text(
        &mut self,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        logical_height: u32,
    ) -> Result<(), FontError> {
        let layout = layout_live_quads(
            live,
            geometry,
            f64::from(logical_height) / 768.0,
            &self.glyphs,
            &self.placements,
            &self.metrics,
            self.extent,
        )?;
        self.live_quads = layout.quads;
        self.edit_box_layouts = layout.edit_boxes;
        Ok(())
    }

    /// Recolors retained button-label face and shadow passes without laying out glyphs.
    pub(crate) fn refresh_live_text_colors(&mut self, changes: &[UiRuntimeTextColorChange]) {
        for change in changes {
            let previous_color = change.previous_color.map(|component| component as f32);
            let color = change.color.map(|component| component as f32);
            let previous_shadow = change
                .previous_shadow_color
                .map(|component| component as f32);
            let shadow = change.shadow_color.map(|component| component as f32);
            for quad in self
                .live_quads
                .iter_mut()
                .filter(|quad| quad.object_index == change.object_index)
            {
                if quad.color == previous_color {
                    quad.color = color;
                } else if quad.color == previous_shadow {
                    quad.color = shadow;
                }
            }
        }
    }

    /// Returns retained glyph corner colors for one live text object in mesh order.
    pub(crate) fn retained_object_colors(
        &self,
        object_index: usize,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: &UiScrollFramePlan,
    ) -> Vec<[[f32; 4]; 4]> {
        self.live_quads
            .iter()
            .filter(|quad| quad.object_index == object_index)
            .filter_map(|quad| {
                if quad
                    .clip_object
                    .and_then(|clip| scroll_frames.state(clip))
                    .is_some()
                {
                    resolve_quad_unclipped(quad, geometry, None)
                } else {
                    resolve_quad(quad, geometry, None)
                }
            })
            .map(|quad| [quad.color(); 4])
            .collect()
    }

    /// Maps one presentation-space pointer to the nearest UTF-8 insertion boundary.
    ///
    /// The retained cluster advances are the same values used to draw the
    /// EditBox. This keeps pointer placement exact for proportional and password
    /// text without reconstructing font metrics in the input owner.
    pub(crate) fn edit_box_cursor_at(
        &self,
        geometry: &UiRegionGeometryPlan,
        object_index: usize,
        point: (f64, f64),
    ) -> Option<usize> {
        let layout = self.edit_box_layouts.get(object_index)?.as_ref()?;
        let region = geometry.region(object_index)?;
        let owner = region.presentation_bounds();
        let scale = region.effective_scale();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let local_x = (point.0 - owner.left()) / scale;
        let local_y = (point.1 - owner.top()) / scale;
        layout.cursor_at((local_x, local_y))
    }

    /// Returns the stable selection anchor represented by one live EditBox.
    pub(crate) fn edit_box_selection_anchor(&self, object_index: usize) -> Option<usize> {
        self.edit_box_layouts
            .get(object_index)?
            .as_ref()
            .map(EditBoxTextLayout::selection_anchor)
    }

    /// Returns the process-local immutable atlas identity.
    #[must_use]
    pub const fn identity(&self) -> u64 {
        self.identity
    }

    /// Returns the coverage texture dimensions.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Returns tightly packed linear RGBA8 pixels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Resolves visible glyph quads in object, line, and character order.
    ///
    /// The immutable atlas and local layout survive Glue screen changes. Only
    /// inherited visibility, scale, alpha, and screen placement are applied
    /// here, so revealing legal copy after the first-run movie does not touch
    /// FreeType or rebuild coverage pixels.
    #[must_use]
    pub fn quads(&self, geometry: &UiRegionGeometryPlan) -> Vec<UiGlyphQuad> {
        self.resolve_quads(geometry, None)
    }

    /// Resolves visible glyphs after applying live ScrollFrame child offsets.
    #[must_use]
    pub fn quads_with_scroll(
        &self,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: &UiScrollFramePlan,
    ) -> Vec<UiGlyphQuad> {
        self.resolve_quads(geometry, Some(scroll_frames))
    }

    /// Resolves the complete immutable glyph mesh before GPU scroll translation.
    ///
    /// Rectangular ScrollFrame clipping is draw state, so every document line
    /// remains resident and scrolling never has to regenerate glyph vertices.
    pub(crate) fn retained_scroll_quads(
        &self,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: &UiScrollFramePlan,
    ) -> Vec<UiGlyphQuad> {
        self.html_quads
            .iter()
            .chain(&self.live_quads)
            .filter_map(|quad| {
                if quad
                    .clip_object
                    .and_then(|clip| scroll_frames.state(clip))
                    .is_some()
                {
                    resolve_quad_unclipped(quad, geometry, None)
                } else {
                    resolve_quad(quad, geometry, None)
                }
            })
            .collect()
    }

    fn resolve_quads(
        &self,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: Option<&UiScrollFramePlan>,
    ) -> Vec<UiGlyphQuad> {
        let mut resolved = Vec::new();
        for run in &self.html_runs {
            if !run.is_visible(&self.html_quads, geometry, scroll_frames) {
                continue;
            }
            resolved.extend(
                self.html_quads[run.first_quad..run.first_quad + run.quad_count]
                    .iter()
                    .filter_map(|quad| resolve_quad(quad, geometry, scroll_frames)),
            );
        }
        resolved.extend(
            self.live_quads
                .iter()
                .filter_map(|quad| resolve_quad(quad, geometry, scroll_frames)),
        );
        resolved
    }
}

/// One glyph positioned relative to the top-left of its `SimpleHTML` owner.
#[derive(Clone, Debug, PartialEq)]
struct LocalGlyphQuad {
    packet_key: Option<UiPresentationPacketKey>,
    object_index: usize,
    clip_object: Option<usize>,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    color: [f32; 4],
    caret: bool,
}

/// Contiguous glyphs from one immutable `SimpleHTML` line.
///
/// The line envelope lets scrolling reject almost the entire legal document
/// before touching individual glyphs. Glyphs in the few intersecting lines
/// still take the exact per-quad clipping path, including partial edge rows.
#[derive(Clone, Debug, PartialEq)]
struct LocalGlyphRun {
    first_quad: usize,
    quad_count: usize,
    bounds: [f32; 4],
}

impl LocalGlyphRun {
    fn is_visible(
        &self,
        quads: &[LocalGlyphQuad],
        geometry: &UiRegionGeometryPlan,
        scroll_frames: Option<&UiScrollFramePlan>,
    ) -> bool {
        let Some(quad) = quads.get(self.first_quad) else {
            return false;
        };
        let Some(region) = geometry.region(quad.object_index) else {
            return false;
        };
        if !region.effectively_shown()
            || region.effective_alpha() <= 0.0 && !region.animation_active()
        {
            return false;
        }
        let Some(clip_object) = quad.clip_object else {
            return true;
        };
        let Some(clip) = geometry.region(clip_object) else {
            return false;
        };
        if !clip.effectively_shown() || clip.effective_alpha() <= 0.0 && !clip.animation_active() {
            return false;
        }
        let owner = region.presentation_bounds();
        let viewport = clip.presentation_bounds();
        let scale = region.effective_scale();
        let scroll = scroll_frames
            .and_then(|frames| frames.state(clip_object))
            .map_or((0.0, 0.0), crate::UiScrollFrameState::offset);
        let [left, bottom, right, top] = self.bounds.map(f64::from);
        let resolved_left = owner.left() + (left - scroll.0) * scale;
        let resolved_bottom = owner.top() + (bottom + scroll.1) * scale;
        let resolved_right = owner.left() + (right - scroll.0) * scale;
        let resolved_top = owner.top() + (top + scroll.1) * scale;
        resolved_left < viewport.right()
            && resolved_right > viewport.left()
            && resolved_bottom < viewport.top()
            && resolved_top > viewport.bottom()
    }
}

#[derive(Default)]
struct HtmlGlyphLayout {
    quads: Vec<LocalGlyphQuad>,
    runs: Vec<LocalGlyphRun>,
}

/// Text geometry retained for EditBox pointer placement and drag selection.
#[derive(Clone, Debug, Default, PartialEq)]
struct LiveTextLayout {
    quads: Vec<LocalGlyphQuad>,
    edit_boxes: Vec<Option<EditBoxTextLayout>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GlyphKey {
    face: AssetPath,
    pixel_height: u32,
    rasterization: FontRasterization,
    character: char,
}

impl GlyphKey {
    fn new(font: &LineFontKey, character: char) -> Self {
        Self {
            face: font.face.clone(),
            pixel_height: font.pixel_height,
            rasterization: font.rasterization,
            character,
        }
    }

    fn font(&self) -> LineFontKey {
        LineFontKey {
            face: self.face.clone(),
            pixel_height: self.pixel_height,
            rasterization: self.rasterization,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LineFontKey {
    face: AssetPath,
    pixel_height: u32,
    rasterization: FontRasterization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FontMetrics {
    ascender_26_6: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AtlasPlacement {
    x: u32,
    y: u32,
}

type PackedAtlas = ((u32, u32), HashMap<GlyphKey, AtlasPlacement>);

fn font_definition<'a>(
    fonts: &'a FontCatalog,
    name: &str,
) -> Result<&'a crate::FontDefinition, FontError> {
    fonts
        .definition(name)
        .ok_or_else(|| FontError::Presentation {
            message: format!("font object {name} is unavailable"),
        })
}

fn font_key(
    definition: &crate::FontDefinition,
    pixels_per_ui_unit: f64,
) -> Result<LineFontKey, FontError> {
    let face = definition
        .face()
        .cloned()
        .ok_or_else(|| FontError::Presentation {
            message: format!("font object {} has no face", definition.name()),
        })?;
    let height = definition.height().ok_or_else(|| FontError::Presentation {
        message: format!("font object {} has no height", definition.name()),
    })?;
    Ok(LineFontKey {
        face,
        pixel_height: (f64::from(height) * pixels_per_ui_unit).round().max(1.0) as u32,
        rasterization: if definition.monochrome().unwrap_or(false) {
            FontRasterization::Monochrome
        } else {
            FontRasterization::Antialiased
        },
    })
}

fn runtime_font_key(
    text: &UiRuntimeText,
    pixels_per_ui_unit: f64,
) -> Result<LineFontKey, FontError> {
    if !pixels_per_ui_unit.is_finite() || pixels_per_ui_unit <= 0.0 {
        return Err(FontError::Presentation {
            message: "text raster scale is not positive and finite".to_owned(),
        });
    }
    Ok(LineFontKey {
        face: text.face.clone(),
        pixel_height: (text.height * pixels_per_ui_unit).round().max(1.0) as u32,
        rasterization: text.rasterization,
    })
}

fn native_font_key(
    style: &UiNativeTextStyle,
    pixels_per_ui_unit: f64,
) -> Result<LineFontKey, FontError> {
    if !pixels_per_ui_unit.is_finite()
        || pixels_per_ui_unit <= 0.0
        || !style.height.is_finite()
        || style.height <= 0.0
    {
        return Err(FontError::Presentation {
            message: "native text raster scale is not positive and finite".to_owned(),
        });
    }
    Ok(LineFontKey {
        face: style.face.clone(),
        pixel_height: (f64::from(style.height) * pixels_per_ui_unit)
            .round()
            .max(1.0) as u32,
        rasterization: style.rasterization,
    })
}

fn offset_bounds(mut bounds: [f32; 4], offset: [f32; 2]) -> [f32; 4] {
    bounds[0] += offset[0];
    bounds[1] += offset[1];
    bounds[2] += offset[0];
    bounds[3] += offset[1];
    bounds
}

fn native_glyph_quad(
    identity: u64,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    color: [f32; 4],
) -> UiRenderQuad {
    UiRenderQuad::new(
        0,
        UiRenderSource::GlyphAtlas(identity),
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        bounds,
        texture_coordinates,
        [color; 4],
    )
}

/// Requests the build-12340 western baseline for each active live font.
///
/// The recovered client atlas keeps U+0020 through U+00FF resident. Account,
/// password, realm, and AddOn text therefore does not rerasterize during
/// ordinary Latin input; localized scalars outside that set are added exactly
/// when they occur in the loaded Glue state.
fn request_live_glyphs(
    live: &UiRuntimeObjectPlan,
    pixels_per_ui_unit: f64,
    requested: &mut HashSet<GlyphKey>,
    required: &mut HashSet<GlyphKey>,
) -> Result<(), FontError> {
    for text in live
        .objects()
        .iter()
        .filter_map(|object| object.text.as_ref())
    {
        let font = runtime_font_key(text, pixels_per_ui_unit)?;
        for value in 0x20..=0xff {
            if let Some(character) = char::from_u32(value) {
                requested.insert(GlyphKey::new(&font, character));
            }
        }
        for presented in presented_characters(text)
            .into_iter()
            .filter(|presented| !presented.character.is_control())
        {
            let character = presented.character;
            let key = GlyphKey::new(&font, character);
            requested.insert(key.clone());
            required.insert(key);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PresentedCharacter {
    character: char,
    color: Option<[f32; 4]>,
    source_begin: usize,
    source_end: usize,
}

/// Wraps one explicit line at whitespace boundaries using stock glyph advances.
///
/// `CSimpleFontString::LoadXML` at `0x004873E0` leaves word wrapping enabled
/// unless the `wordwrap` attribute disables it. The same path admits
/// `nonspacewrap` for words wider than the field. Leading and boundary
/// whitespace is not carried onto a continuation line.
pub(crate) fn wrap_line<T: Copy>(
    items: &[T],
    max_width: f64,
    non_space_wrap: bool,
    character: impl Fn(&T) -> char,
    advance: impl Fn(&T) -> f64,
) -> Vec<Vec<T>> {
    if items.is_empty() || !max_width.is_finite() || max_width <= 0.0 {
        return vec![items.to_vec()];
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0.0;
    let mut cursor = 0;
    while cursor < items.len() {
        let whitespace_start = cursor;
        while cursor < items.len() && character(&items[cursor]).is_whitespace() {
            cursor += 1;
        }
        if cursor == items.len() {
            break;
        }
        let word_start = cursor;
        while cursor < items.len() && !character(&items[cursor]).is_whitespace() {
            cursor += 1;
        }
        let whitespace = &items[whitespace_start..word_start];
        let word = &items[word_start..cursor];
        let whitespace_width = whitespace.iter().map(&advance).sum::<f64>();
        let word_width = word.iter().map(&advance).sum::<f64>();
        let joined_width = current_width
            + if current.is_empty() {
                0.0
            } else {
                whitespace_width
            }
            + word_width;
        if !current.is_empty() && joined_width > max_width {
            lines.push(std::mem::take(&mut current));
            current_width = 0.0;
        } else if !current.is_empty() {
            current.extend_from_slice(whitespace);
            current_width += whitespace_width;
        }
        if non_space_wrap && word_width > max_width {
            for item in word {
                let item_width = advance(item);
                if !current.is_empty() && current_width + item_width > max_width {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0.0;
                }
                current.push(*item);
                current_width += item_width;
            }
        } else {
            current.extend_from_slice(word);
            current_width += word_width;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Removes the inline formatting vocabulary consumed by build-12340
/// FontStrings while retaining the active color on each visible scalar.
fn presented_characters(text: &UiRuntimeText) -> Vec<PresentedCharacter> {
    let mut output = Vec::with_capacity(text.content.chars().count());
    let mut cursor = 0_usize;
    let mut color = None;
    while cursor < text.content.len() {
        let remaining = &text.content[cursor..];
        if let Some(hex) = remaining.strip_prefix("|c").and_then(|tail| tail.get(..8))
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            let bytes = hex.as_bytes();
            let component = |offset: usize| {
                f32::from(hex_nibble(bytes[offset]) * 16 + hex_nibble(bytes[offset + 1])) / 255.0
            };
            color = Some([component(2), component(4), component(6), component(0)]);
            cursor += 10;
            continue;
        }
        if remaining.starts_with("|r") {
            color = None;
            cursor += 2;
            continue;
        }
        if remaining.starts_with("|n") {
            output.push(PresentedCharacter {
                character: '\n',
                color,
                source_begin: cursor,
                source_end: cursor + 2,
            });
            cursor += 2;
            continue;
        }
        if remaining.starts_with("||") {
            output.push(PresentedCharacter {
                character: if text.password { '*' } else { '|' },
                color,
                source_begin: cursor,
                source_end: cursor + 2,
            });
            cursor += 2;
            continue;
        }
        let Some(character) = remaining.chars().next() else {
            break;
        };
        let source_begin = cursor;
        cursor += character.len_utf8();
        output.push(PresentedCharacter {
            character: if text.password && !matches!(character, ' ' | '\t' | '\r' | '\n') {
                '*'
            } else {
                character
            },
            color,
            source_begin,
            source_end: cursor,
        });
    }
    output
}

const fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_live_quads(
    live: &UiRuntimeObjectPlan,
    geometry: &UiRegionGeometryPlan,
    pixels_per_ui_unit: f64,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
    metrics: &HashMap<LineFontKey, FontMetrics>,
    extent: (u32, u32),
) -> Result<LiveTextLayout, FontError> {
    let mut quads = Vec::new();
    let mut edit_boxes = vec![None; live.objects().len()];
    for (object_index, object) in live.objects().iter().enumerate() {
        let Some(text) = &object.text else {
            continue;
        };
        if text.content.is_empty()
            && !(object.kind == UiObjectKind::EditBox && object.edit_focused.unwrap_or(false))
        {
            continue;
        }
        let Some(region) = geometry.region(object_index) else {
            continue;
        };
        let font = runtime_font_key(text, pixels_per_ui_unit)?;
        let packet_key = UiPresentationPacketKey::for_text(live, object_index);
        let metrics = metrics.get(&font).ok_or_else(|| FontError::Presentation {
            message: format!(
                "live text object {object_index} has no retained metrics for {} at {}px",
                font.face, font.pixel_height
            ),
        })?;
        let displayed = presented_characters(text);
        let lines = if object.kind == UiObjectKind::EditBox && !text.multiline {
            vec![
                displayed
                    .into_iter()
                    .filter(|presented| !matches!(presented.character, '\r' | '\n'))
                    .collect::<Vec<_>>(),
            ]
        } else {
            let mut lines = vec![Vec::new()];
            let mut previous_was_carriage_return = false;
            for presented in displayed {
                if presented.character == '\r' {
                    lines.push(Vec::new());
                    previous_was_carriage_return = true;
                } else if presented.character == '\n' {
                    if !previous_was_carriage_return {
                        lines.push(Vec::new());
                    }
                    previous_was_carriage_return = false;
                } else if let Some(line) = lines.last_mut() {
                    line.push(presented);
                    previous_was_carriage_return = false;
                }
            }
            lines
        };
        let mut lines = if lines.is_empty() {
            vec![Vec::new()]
        } else {
            lines
        };
        let owner = region.logical_bounds();
        let [inset_left, inset_right, inset_top, inset_bottom] = text.text_insets;
        let available_width = (owner.width() - inset_left - inset_right).max(0.0);
        let available_height = (owner.height() - inset_top - inset_bottom).max(0.0);
        // CSimpleButton owns a single-line label even though the nested
        // FontString begins with CSimpleFontString's ordinary wrap default.
        // Stock clips a long label to the button instead of creating a second
        // line; CharSelectCreateCharacterButton exposes the distinction.
        if object.kind == UiObjectKind::FontString
            && object.role != UiObjectRole::ButtonText
            && text.word_wrap
        {
            lines = lines
                .into_iter()
                .flat_map(|line| {
                    wrap_line(
                        &line,
                        available_width,
                        text.non_space_wrap,
                        |presented| presented.character,
                        |presented| {
                            glyphs
                                .get(&GlyphKey::new(&font, presented.character))
                                .map_or(0.0, |glyph| {
                                    glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit
                                })
                        },
                    )
                })
                .collect();
        }
        if text.max_lines > 0 {
            lines.truncate(text.max_lines as usize);
        }
        let line_height = f64::from(font.pixel_height) / pixels_per_ui_unit;
        let block_height =
            line_height * lines.len() as f64 + text.spacing * lines.len().saturating_sub(1) as f64;
        let block_top = match text.vertical {
            crate::VerticalJustification::Top => -inset_top,
            crate::VerticalJustification::Middle => {
                -inset_top - (available_height - block_height).max(0.0) * 0.5
            }
            crate::VerticalJustification::Bottom => -owner.height() + inset_bottom + block_height,
        };
        let ascender = metrics.ascender_26_6 as f64 / 64.0 / pixels_per_ui_unit;
        let color = text.color.map(|component| component as f32);
        // CSimpleScrollFrame clips every region beneath its assigned child,
        // not only SimpleHTML. CharacterCreate's race and class descriptions
        // are ordinary FontStrings and depend on this viewport inheritance.
        let clip_object = if object.kind == UiObjectKind::EditBox {
            Some(object_index)
        } else {
            nearest_owning_scroll_frame(live, object_index)
        };
        let mut primary_quads = Vec::new();
        let mut selection_quads = Vec::new();
        let mut caret_quads = Vec::new();
        let mut edit_box_layout = (object.kind == UiObjectKind::EditBox)
            .then(|| EditBoxTextLayout::new(text.content.len(), text.cursor, text.selection));
        if let Some(layout) = edit_box_layout.as_mut() {
            layout.reserve_lines(lines.len());
        }
        for (line_index, line) in lines.iter().enumerate() {
            let line_width = line.iter().try_fold(0.0, |width, presented| {
                let character = presented.character;
                let key = GlyphKey::new(&font, character);
                glyphs.get(&key).map_or_else(
                    || {
                        Err(FontError::Presentation {
                            message: format!(
                                "live text object {object_index} uses unavailable glyph {character:?}"
                            ),
                        })
                    },
                    |glyph| {
                        Ok(width
                            + glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit)
                    },
                )
            })?;
            let mut pen_x = match text.horizontal {
                crate::HorizontalJustification::Left => inset_left,
                crate::HorizontalJustification::Center => {
                    inset_left + (available_width - line_width) * 0.5
                }
                crate::HorizontalJustification::Right => owner.width() - inset_right - line_width,
            };
            let line_top = block_top - line_index as f64 * (line_height + text.spacing);
            let baseline = line_top - ascender;
            let caret = (object.kind == UiObjectKind::EditBox
                && object.edit_focused.unwrap_or(false)
                && !text.multiline
                && line_index == 0)
                .then(|| {
                    edit_box_caret_metrics(text, &font, glyphs, pixels_per_ui_unit, available_width)
                });
            if let Some((caret_offset, caret_width)) = caret {
                // CSimpleEditBox scrolls its one-line text just enough to keep
                // the insertion cell inside the authored text insets.
                let caret_left = pen_x + caret_offset;
                let content_right = owner.width() - inset_right;
                if caret_left + caret_width > content_right {
                    pen_x += content_right - caret_width - caret_left;
                } else if caret_left < inset_left {
                    pen_x += inset_left - caret_left;
                }
            }
            let line_start_x = pen_x;
            if let Some(layout) = edit_box_layout.as_mut() {
                layout.push_line(baseline);
            }
            let selection_begin = text.selection[0].min(text.selection[1]);
            let selection_end = text.selection[0].max(text.selection[1]);
            for presented in line {
                let character = presented.character;
                let key = GlyphKey::new(&font, character);
                let glyph = glyphs.get(&key).ok_or_else(|| FontError::Presentation {
                    message: format!(
                        "live text object {object_index} uses unavailable glyph {character:?}"
                    ),
                })?;
                let advance = glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit;
                if let Some(layout) = edit_box_layout.as_mut() {
                    layout.push_cluster(
                        presented.source_begin,
                        presented.source_end,
                        line_index,
                        pen_x,
                        pen_x + advance,
                    );
                }
                if selection_begin != selection_end
                    && presented.source_end > selection_begin
                    && presented.source_begin < selection_end
                    && advance > 0.0
                {
                    selection_quads.push(LocalGlyphQuad {
                        packet_key,
                        object_index,
                        clip_object,
                        bounds: [
                            pen_x as f32,
                            (line_top - line_height) as f32,
                            (pen_x + advance) as f32,
                            line_top as f32,
                        ],
                        texture_coordinates: solid_coordinates(extent),
                        color: text.highlight_color.map(|component| component as f32),
                        caret: false,
                    });
                }
                if glyph.width() > 0 && glyph.height() > 0 {
                    let placement = placements.get(&key).ok_or_else(|| {
                        FontError::Presentation {
                            message: format!(
                                "live text object {object_index} has no atlas placement for {character:?}"
                            ),
                        }
                    })?;
                    let left = pen_x + f64::from(glyph.bearing_x()) / pixels_per_ui_unit;
                    let top = baseline + f64::from(glyph.bearing_y()) / pixels_per_ui_unit;
                    let right = left + f64::from(glyph.width()) / pixels_per_ui_unit;
                    let bottom = top - f64::from(glyph.height()) / pixels_per_ui_unit;
                    let u0 = placement.x as f32 / extent.0 as f32;
                    let v0 = placement.y as f32 / extent.1 as f32;
                    let u1 = (placement.x + glyph.width()) as f32 / extent.0 as f32;
                    let v1 = (placement.y + glyph.height()) as f32 / extent.1 as f32;
                    primary_quads.push(LocalGlyphQuad {
                        packet_key,
                        object_index,
                        clip_object,
                        bounds: [left as f32, bottom as f32, right as f32, top as f32],
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color: presented.color.unwrap_or(color),
                        caret: false,
                    });
                }
                pen_x += advance;
            }
            if let Some((caret_offset, caret_width)) = caret {
                // Build 12340 presents the insertion point as a full
                // character-cell block. Pixel (0, 0) is the atlas-owned solid
                // coverage sample reserved by `compose_atlas`. Retain that
                // final quad while the blink is dark so a half-cycle changes
                // four color vertices instead of the complete material and
                // draw topology of the Glue frame.
                caret_quads.push(LocalGlyphQuad {
                    packet_key,
                    object_index,
                    clip_object,
                    bounds: [
                        (line_start_x + caret_offset) as f32,
                        (line_top - line_height) as f32,
                        (line_start_x + caret_offset + caret_width) as f32,
                        line_top as f32,
                    ],
                    texture_coordinates: solid_coordinates(extent),
                    color,
                    caret: true,
                });
            }
        }
        // GxuFontString draws material passes in outline, shadow, then face
        // order. Keeping these as separate quads restores the black edging
        // and offset shadow that make Glue labels legible over animated M2s.
        quads.extend(selection_quads);
        let outline = text.outline_width as f32;
        if outline > 0.0 {
            for primary in &primary_quads {
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
                    quads.push(offset_live_quad(primary, offset, [0.0, 0.0, 0.0, 1.0]));
                }
            }
        }
        let shadow_offset = [text.shadow_offset[0] as f32, text.shadow_offset[1] as f32];
        if shadow_offset != [0.0, 0.0] && text.shadow_color[3] > 0.0 {
            let shadow_color = text.shadow_color.map(|component| component as f32);
            quads.extend(
                primary_quads
                    .iter()
                    .map(|primary| offset_live_quad(primary, shadow_offset, shadow_color)),
            );
        }
        quads.extend(primary_quads);
        quads.extend(caret_quads);
        edit_boxes[object_index] = edit_box_layout;
    }
    Ok(LiveTextLayout { quads, edit_boxes })
}

fn solid_coordinates(extent: (u32, u32)) -> [[f32; 2]; 4] {
    [[0.5 / extent.0 as f32, 0.5 / extent.1 as f32]; 4]
}

/// Reuses one glyph's atlas coverage for an offset live-text material pass.
fn offset_live_quad(source: &LocalGlyphQuad, offset: [f32; 2], color: [f32; 4]) -> LocalGlyphQuad {
    LocalGlyphQuad {
        packet_key: source.packet_key,
        object_index: source.object_index,
        clip_object: source.clip_object,
        bounds: offset_bounds(source.bounds, offset),
        texture_coordinates: source.texture_coordinates,
        color,
        caret: source.caret,
    }
}

/// Resolves the byte-indexed EditBox cursor to its visible insertion cell.
fn edit_box_caret_metrics(
    text: &UiRuntimeText,
    font: &LineFontKey,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    pixels_per_ui_unit: f64,
    available_width: f64,
) -> (f64, f64) {
    let mut cursor = text.cursor.min(text.content.len());
    while !text.content.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let presented = |character: char| {
        if text.password && !matches!(character, ' ' | '\t' | '\r' | '\n') {
            '*'
        } else {
            character
        }
    };
    let advance = |character| {
        glyphs
            .get(&GlyphKey::new(font, presented(character)))
            .map_or(0.0, |glyph| {
                glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit
            })
    };
    let offset = text.content[..cursor]
        .chars()
        .filter(|character| text.multiline || !matches!(character, '\r' | '\n'))
        .map(advance)
        .sum();
    let width = text.content[cursor..]
        .chars()
        .find(|character| text.multiline || !matches!(character, '\r' | '\n'))
        .map_or_else(|| advance(' '), advance)
        .max(pixels_per_ui_unit.recip())
        .min(available_width.max(0.0));
    (offset, width)
}

fn pack(
    keys: &[GlyphKey],
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
) -> Result<PackedAtlas, FontError> {
    let widest = keys
        .iter()
        .filter_map(|key| glyphs.get(key))
        .map(|glyph| glyph.width().saturating_add(GLYPH_PADDING * 2))
        .max()
        .unwrap_or(1);
    let width = ATLAS_ROW_WIDTH.max(widest.next_power_of_two());
    let mut x = GLYPH_PADDING;
    let mut y = GLYPH_PADDING;
    let mut row_height = 0_u32;
    let mut placements = HashMap::with_capacity(keys.len());
    for key in keys {
        let glyph = &glyphs[key];
        if glyph.width() == 0 || glyph.height() == 0 {
            placements.insert(key.clone(), AtlasPlacement { x: 0, y: 0 });
            continue;
        }
        let padded_width = glyph.width().saturating_add(GLYPH_PADDING * 2);
        if x.saturating_add(padded_width) > width {
            x = GLYPH_PADDING;
            y = y
                .checked_add(row_height.saturating_add(GLYPH_PADDING * 2))
                .ok_or_else(atlas_overflow)?;
            row_height = 0;
        }
        placements.insert(key.clone(), AtlasPlacement { x, y });
        x = x.checked_add(padded_width).ok_or_else(atlas_overflow)?;
        row_height = row_height.max(glyph.height());
    }
    let used_height = y
        .checked_add(row_height)
        .and_then(|value| value.checked_add(GLYPH_PADDING))
        .ok_or_else(atlas_overflow)?;
    Ok(((width, used_height.max(1).next_power_of_two()), placements))
}

fn compose_atlas(
    extent: (u32, u32),
    keys: &[GlyphKey],
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
) -> Result<Vec<u8>, FontError> {
    let byte_count = u64::from(extent.0)
        .checked_mul(u64::from(extent.1))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(atlas_overflow)?;
    let mut rgba8 = vec![0; byte_count];
    // The first padding texel is dedicated solid coverage for retained text
    // material primitives such as the stock EditBox insertion block.
    rgba8[..4].copy_from_slice(&[255; 4]);
    for key in keys {
        let glyph = &glyphs[key];
        let placement = placements[key];
        for row in 0..glyph.height() {
            for column in 0..glyph.width() {
                let source = (row * glyph.width() + column) as usize;
                let destination =
                    (((placement.y + row) * extent.0 + placement.x + column) * 4) as usize;
                rgba8[destination..destination + 4].copy_from_slice(&[
                    255,
                    255,
                    255,
                    glyph.coverage()[source],
                ]);
            }
        }
    }
    Ok(rgba8)
}

#[allow(clippy::too_many_arguments)]
fn layout_quads(
    html: &UiSimpleHtmlPlan,
    live: Option<&UiRuntimeObjectPlan>,
    geometry: &UiRegionGeometryPlan,
    fonts: &FontCatalog,
    assets: &mut AssetStore,
    pixels_per_ui_unit: f64,
    system: &mut FontSystem,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
    extent: (u32, u32),
) -> Result<HtmlGlyphLayout, FontError> {
    let mut layout = HtmlGlyphLayout::default();
    for object_index in 0..geometry.region_count() {
        let Some(region) = geometry.region(object_index) else {
            continue;
        };
        let Some(node) = html.node(object_index) else {
            continue;
        };
        let width = region.logical_bounds().width();
        for line in node.lines() {
            let first_quad = layout.quads.len();
            let mut run_bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            let definition = font_definition(fonts, line.font_object())?;
            let font = font_key(definition, pixels_per_ui_unit)?;
            let line_width = f64::from(line.width());
            let start = match line.alignment() {
                UiSimpleHtmlAlignment::Left => 0.0,
                UiSimpleHtmlAlignment::Center => (width - line_width) * 0.5,
                UiSimpleHtmlAlignment::Right => width - line_width,
            };
            let ascender = system.ascender_26_6(assets, &font.face, font.pixel_height)? as f64
                / 64.0
                / pixels_per_ui_unit;
            let baseline = -f64::from(line.top()) - ascender;
            let color = definition.color().map_or([1.0; 4], |color| {
                [
                    color.red(),
                    color.green(),
                    color.blue(),
                    color.alpha().unwrap_or(1.0),
                ]
            });
            let mut pen_x_26_6 = 0_i64;
            for character in line.text().chars() {
                let key = GlyphKey {
                    face: font.face.clone(),
                    pixel_height: font.pixel_height,
                    rasterization: font.rasterization,
                    character,
                };
                let glyph = &glyphs[&key];
                if glyph.width() > 0 && glyph.height() > 0 {
                    let placement = placements[&key];
                    let left = start
                        + (pen_x_26_6 as f64 / 64.0 + f64::from(glyph.bearing_x()))
                            / pixels_per_ui_unit;
                    let top = baseline + f64::from(glyph.bearing_y()) / pixels_per_ui_unit;
                    let right = left + f64::from(glyph.width()) / pixels_per_ui_unit;
                    let bottom = top - f64::from(glyph.height()) / pixels_per_ui_unit;
                    let u0 = placement.x as f32 / extent.0 as f32;
                    let v0 = placement.y as f32 / extent.1 as f32;
                    let u1 = (placement.x + glyph.width()) as f32 / extent.0 as f32;
                    let v1 = (placement.y + glyph.height()) as f32 / extent.1 as f32;
                    let bounds = [left as f32, bottom as f32, right as f32, top as f32];
                    run_bounds[0] = run_bounds[0].min(bounds[0]);
                    run_bounds[1] = run_bounds[1].min(bounds[1]);
                    run_bounds[2] = run_bounds[2].max(bounds[2]);
                    run_bounds[3] = run_bounds[3].max(bounds[3]);
                    layout.quads.push(LocalGlyphQuad {
                        packet_key: live
                            .and_then(|live| UiPresentationPacketKey::for_text(live, object_index)),
                        object_index,
                        clip_object: node.clip_object(),
                        bounds,
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color,
                        caret: false,
                    });
                }
                pen_x_26_6 += glyph.advance_x_26_6();
            }
            let quad_count = layout.quads.len() - first_quad;
            if quad_count > 0 {
                layout.runs.push(LocalGlyphRun {
                    first_quad,
                    quad_count,
                    bounds: run_bounds,
                });
            }
        }
    }
    Ok(layout)
}

fn resolve_quad(
    quad: &LocalGlyphQuad,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: Option<&UiScrollFramePlan>,
) -> Option<UiGlyphQuad> {
    let resolved = resolve_quad_unclipped(quad, geometry, scroll_frames)?;
    clip_quad(resolved, quad.clip_object, geometry)
}

fn resolve_quad_unclipped(
    quad: &LocalGlyphQuad,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: Option<&UiScrollFramePlan>,
) -> Option<UiGlyphQuad> {
    let region = geometry.region(quad.object_index)?;
    if !region.effectively_shown() || region.effective_alpha() <= 0.0 && !region.animation_active()
    {
        return None;
    }
    let owner = region.presentation_bounds();
    let scale = region.effective_scale();
    let scroll = quad
        .clip_object
        .and_then(|index| scroll_frames?.state(index))
        .map_or((0.0, 0.0), crate::UiScrollFrameState::offset);
    let [left, bottom, right, top] = quad.bounds;
    let resolved = UiGlyphQuad {
        packet_key: quad.packet_key,
        object_index: quad.object_index,
        clip_object: quad.clip_object,
        bounds: [
            (owner.left() + (f64::from(left) - scroll.0) * scale) as f32,
            (owner.top() + (f64::from(bottom) + scroll.1) * scale) as f32,
            (owner.left() + (f64::from(right) - scroll.0) * scale) as f32,
            (owner.top() + (f64::from(top) + scroll.1) * scale) as f32,
        ],
        texture_coordinates: quad.texture_coordinates,
        color: quad.color,
        caret: quad.caret,
    };
    Some(resolved)
}

/// Clips a glyph to its scroll viewport and retains texel-to-edge alignment.
fn clip_quad(
    mut quad: UiGlyphQuad,
    clip_object: Option<usize>,
    geometry: &UiRegionGeometryPlan,
) -> Option<UiGlyphQuad> {
    let Some(clip_object) = clip_object else {
        return Some(quad);
    };
    let clip = geometry.region(clip_object)?;
    if !clip.effectively_shown() || clip.effective_alpha() <= 0.0 && !clip.animation_active() {
        return None;
    }
    let viewport = clip.presentation_bounds();
    let [left, bottom, right, top] = quad.bounds;
    let clipped_left = left.max(viewport.left() as f32);
    let clipped_bottom = bottom.max(viewport.bottom() as f32);
    let clipped_right = right.min(viewport.right() as f32);
    let clipped_top = top.min(viewport.top() as f32);
    if clipped_left >= clipped_right || clipped_bottom >= clipped_top {
        return None;
    }

    let horizontal_span = right - left;
    let vertical_span = top - bottom;
    let [upper_left, lower_left, upper_right, _lower_right] = quad.texture_coordinates;
    let left_fraction = (clipped_left - left) / horizontal_span;
    let right_fraction = (clipped_right - left) / horizontal_span;
    let top_fraction = (top - clipped_top) / vertical_span;
    let bottom_fraction = (top - clipped_bottom) / vertical_span;
    let u0 = upper_left[0] + (upper_right[0] - upper_left[0]) * left_fraction;
    let u1 = upper_left[0] + (upper_right[0] - upper_left[0]) * right_fraction;
    let v0 = upper_left[1] + (lower_left[1] - upper_left[1]) * top_fraction;
    let v1 = upper_left[1] + (lower_left[1] - upper_left[1]) * bottom_fraction;
    quad.bounds = [clipped_left, clipped_bottom, clipped_right, clipped_top];
    quad.texture_coordinates = [[u0, v0], [u0, v1], [u1, v0], [u1, v1]];
    Some(quad)
}

fn raster_rank(rasterization: FontRasterization) -> u8 {
    match rasterization {
        FontRasterization::Antialiased => 0,
        FontRasterization::Monochrome => 1,
    }
}

fn atlas_overflow() -> FontError {
    FontError::Presentation {
        message: "glyph atlas dimensions overflow".to_owned(),
    }
}

fn next_identity() -> u64 {
    static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);
    NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(advance: i64) -> RasterizedGlyph {
        RasterizedGlyph {
            width: 1,
            height: 1,
            bearing_x: 0,
            bearing_y: 1,
            advance_x_26_6: advance * 64,
            coverage: vec![255],
        }
    }

    #[test]
    fn edit_box_caret_uses_password_cell_and_utf8_cursor_boundary() -> Result<(), FontError> {
        let face = AssetPath::new("Fonts\\FRIZQT__.TTF")?;
        let font = LineFontKey {
            face: face.clone(),
            pixel_height: 12,
            rasterization: FontRasterization::Antialiased,
        };
        let mut glyphs = HashMap::new();
        glyphs.insert(GlyphKey::new(&font, '*'), glyph(7));
        glyphs.insert(GlyphKey::new(&font, ' '), glyph(3));
        let text = UiRuntimeText {
            content: "éx".to_owned(),
            face,
            height: 12.0,
            rasterization: FontRasterization::Antialiased,
            outline_width: 0.0,
            color: [1.0; 4],
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            spacing: 0.0,
            word_wrap: false,
            non_space_wrap: false,
            max_lines: 0,
            horizontal: crate::HorizontalJustification::Left,
            vertical: crate::VerticalJustification::Middle,
            draw_layer: crate::UiDrawLayer::Artwork,
            draw_sub_level: 0,
            password: true,
            multiline: false,
            text_insets: [0.0; 4],
            cursor: 2,
            selection: [2, 2],
            caret_visible: true,
            highlight_color: [96.0 / 255.0, 96.0 / 255.0, 96.0 / 255.0, 1.0],
        };

        assert_eq!(
            edit_box_caret_metrics(&text, &font, &glyphs, 1.0, 100.0),
            (7.0, 7.0)
        );
        Ok(())
    }

    #[test]
    fn atlas_reserves_opaque_padding_texel_for_text_primitives() -> Result<(), FontError> {
        let pixels = compose_atlas((1, 1), &[], &HashMap::new(), &HashMap::new())?;
        assert_eq!(pixels, [255; 4]);
        Ok(())
    }
}
