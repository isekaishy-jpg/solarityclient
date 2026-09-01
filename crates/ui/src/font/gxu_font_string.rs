//! Deduplicated glyph-atlas and quad generation for retained font strings.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::{AssetPath, AssetStore};

use crate::{
    FontCatalog, FontError, FontRasterization, FontSystem, RasterizedGlyph, UiRegionGeometryPlan,
    UiScrollFramePlan, UiSimpleHtmlAlignment, UiSimpleHtmlPlan,
};

const ATLAS_ROW_WIDTH: u32 = 512;
const GLYPH_PADDING: u32 = 1;

/// One positioned glyph sampling the current immutable coverage atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphQuad {
    object_index: usize,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    color: [f32; 4],
}

impl UiGlyphQuad {
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
    local_quads: Vec<LocalGlyphQuad>,
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

        let mut system = FontSystem::new()?;
        let mut keys = requested.into_iter().collect::<Vec<_>>();
        keys.sort_by(|left, right| {
            left.face
                .as_str()
                .cmp(right.face.as_str())
                .then(left.pixel_height.cmp(&right.pixel_height))
                .then(raster_rank(left.rasterization).cmp(&raster_rank(right.rasterization)))
                .then(left.character.cmp(&right.character))
        });
        let mut glyphs = HashMap::with_capacity(keys.len());
        for key in &keys {
            let glyph = system.rasterize(
                assets,
                &key.face,
                key.pixel_height,
                key.character,
                key.rasterization,
            )?;
            glyphs.insert(key.clone(), glyph);
        }
        let (extent, placements) = pack(&keys, &glyphs)?;
        let rgba8 = compose_atlas(extent, &keys, &glyphs, &placements)?;
        let local_quads = layout_quads(
            html,
            geometry,
            fonts,
            assets,
            pixels_per_ui_unit,
            &mut system,
            &glyphs,
            &placements,
            extent,
        )?;
        Ok(Self {
            identity: next_identity(),
            extent,
            rgba8,
            local_quads,
        })
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
        self.local_quads
            .iter()
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

#[derive(Clone, Debug)]
struct LineFontKey {
    face: AssetPath,
    pixel_height: u32,
    rasterization: FontRasterization,
}

#[derive(Clone, Copy, Debug)]
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
            let mut previous = None;
            for character in line.text().chars() {
                if let Some(left) = previous {
                    pen_x_26_6 += system.kerning_x_26_6(
                        assets,
                        &font.face,
                        font.pixel_height,
                        left,
                        character,
                    )?;
                }
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
                        object_index,
                        clip_object: node.clip_object(),
                        bounds: [left as f32, bottom as f32, right as f32, top as f32],
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color,
                    });
                }
                pen_x_26_6 += glyph.advance_x_26_6();
                previous = Some(character);
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
