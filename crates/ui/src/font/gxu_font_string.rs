//! Deduplicated glyph-atlas and quad generation for retained font strings.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::{AssetPath, AssetStore};

use crate::script::{UiRuntimeObjectPlan, UiRuntimeText};
use crate::{
    FontCatalog, FontError, FontRasterization, FontSystem, RasterizedGlyph, UiObjectKind,
    UiPresentationPacketKey, UiRegionGeometryPlan, UiScrollFramePlan, UiSimpleHtmlAlignment,
    UiSimpleHtmlPlan,
};

const ATLAS_ROW_WIDTH: u32 = 512;
const GLYPH_PADDING: u32 = 1;

/// One positioned glyph sampling the current immutable coverage atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphQuad {
    packet_key: Option<UiPresentationPacketKey>,
    object_index: usize,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    color: [f32; 4],
}

impl UiGlyphQuad {
    pub(crate) const fn packet_key(&self) -> Option<UiPresentationPacketKey> {
        self.packet_key
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

    /// Returns the inherited stock font color and effective alpha.
    #[must_use]
    pub const fn color(&self) -> [f32; 4] {
        self.color
    }
}

/// One immutable coverage texture and every glyph quad that samples it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphAtlasPlan {
    identity: u64,
    extent: (u32, u32),
    rgba8: Vec<u8>,
    html_quads: Vec<LocalGlyphQuad>,
    live_quads: Vec<LocalGlyphQuad>,
    glyphs: HashMap<GlyphKey, RasterizedGlyph>,
    placements: HashMap<GlyphKey, AtlasPlacement>,
    metrics: HashMap<LineFontKey, FontMetrics>,
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
        let html_quads = layout_quads(
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
        let live_quads = live.map_or(Ok(Vec::new()), |live| {
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
            html_quads,
            live_quads,
            glyphs,
            placements,
            metrics,
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
                        .filter(|character| !character.is_control())
                        .all(|character| self.glyphs.contains_key(&GlyphKey::new(&font, character)))
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
        self.live_quads = layout_live_quads(
            live,
            geometry,
            f64::from(logical_height) / 768.0,
            &self.glyphs,
            &self.placements,
            &self.metrics,
            self.extent,
        )?;
        Ok(())
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

    fn resolve_quads(
        &self,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: Option<&UiScrollFramePlan>,
    ) -> Vec<UiGlyphQuad> {
        self.html_quads
            .iter()
            .chain(&self.live_quads)
            .filter_map(|quad| {
                let region = geometry.region(quad.object_index)?;
                if !region.effectively_shown() || region.effective_alpha() <= 0.0 {
                    return None;
                }
                let owner = region.presentation_bounds();
                let scale = region.effective_scale();
                let scroll = quad
                    .clip_object
                    .and_then(|index| scroll_frames?.state(index))
                    .map_or((0.0, 0.0), crate::UiScrollFrameState::offset);
                let [left, bottom, right, top] = quad.bounds;
                let mut color = quad.color;
                color[3] *= region.effective_alpha() as f32;
                let resolved = UiGlyphQuad {
                    packet_key: quad.packet_key,
                    object_index: quad.object_index,
                    bounds: [
                        (owner.left() + (f64::from(left) - scroll.0) * scale) as f32,
                        (owner.top() + (f64::from(bottom) + scroll.1) * scale) as f32,
                        (owner.left() + (f64::from(right) - scroll.0) * scale) as f32,
                        (owner.top() + (f64::from(top) + scroll.1) * scale) as f32,
                    ],
                    texture_coordinates: quad.texture_coordinates,
                    color,
                };
                clip_quad(resolved, quad.clip_object, geometry)
            })
            .collect()
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
        for character in presented_characters(text).filter(|character| !character.is_control()) {
            let key = GlyphKey::new(&font, character);
            requested.insert(key.clone());
            required.insert(key);
        }
    }
    Ok(())
}

fn presented_characters(text: &UiRuntimeText) -> impl Iterator<Item = char> + '_ {
    text.content.chars().map(|character| {
        if text.password && !matches!(character, ' ' | '\t' | '\r' | '\n') {
            '*'
        } else {
            character
        }
    })
}

fn presented_text(text: &UiRuntimeText) -> String {
    presented_characters(text).collect()
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
) -> Result<Vec<LocalGlyphQuad>, FontError> {
    let mut quads = Vec::new();
    for (object_index, object) in live.objects().iter().enumerate() {
        let Some(text) = &object.text else {
            continue;
        };
        if text.content.is_empty() {
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
        let displayed = presented_text(text);
        let lines = if object.kind == UiObjectKind::EditBox && !text.multiline {
            vec![displayed.replace(['\r', '\n'], "")]
        } else {
            displayed
                .split(['\r', '\n'])
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        let owner = region.logical_bounds();
        let [inset_left, inset_right, inset_top, inset_bottom] = text.text_insets;
        let available_width = (owner.width() - inset_left - inset_right).max(0.0);
        let available_height = (owner.height() - inset_top - inset_bottom).max(0.0);
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
        for (line_index, line) in lines.iter().enumerate() {
            let line_width = line.chars().try_fold(0.0, |width, character| {
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
            for character in line.chars() {
                let key = GlyphKey::new(&font, character);
                let glyph = glyphs.get(&key).ok_or_else(|| FontError::Presentation {
                    message: format!(
                        "live text object {object_index} uses unavailable glyph {character:?}"
                    ),
                })?;
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
                    quads.push(LocalGlyphQuad {
                        packet_key,
                        object_index,
                        clip_object: (object.kind == UiObjectKind::EditBox).then_some(object_index),
                        bounds: [left as f32, bottom as f32, right as f32, top as f32],
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color,
                    });
                }
                pen_x += glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit;
            }
        }
    }
    Ok(quads)
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
) -> Result<Vec<LocalGlyphQuad>, FontError> {
    let mut quads = Vec::new();
    for object_index in 0..geometry.region_count() {
        let Some(region) = geometry.region(object_index) else {
            continue;
        };
        let Some(node) = html.node(object_index) else {
            continue;
        };
        let width = region.logical_bounds().width();
        for line in node.lines() {
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
                    quads.push(LocalGlyphQuad {
                        packet_key: live
                            .and_then(|live| UiPresentationPacketKey::for_text(live, object_index)),
                        object_index,
                        clip_object: node.clip_object(),
                        bounds: [left as f32, bottom as f32, right as f32, top as f32],
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color,
                    });
                }
                pen_x_26_6 += glyph.advance_x_26_6();
            }
        }
    }
    Ok(quads)
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
    if !clip.effectively_shown() || clip.effective_alpha() <= 0.0 {
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
