//! Batched UI mesh generation recovered from `CSimpleRender.cpp`.

use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiTextureAddressMode,
    UiTextureResidency,
};

use crate::{
    UiBlendMode, UiGlyphAtlasPlan, UiGlyphQuad, UiPresentationPlan, UiRegionGeometryPlan,
    UiRenderError, UiScrollFramePlan, UiTextureAssetPlan, UiTexturePresentation, UiTextureSource,
};

/// Cropped geometry and interpolants retained for one ScrollFrame texture.
struct ClippedTexturedQuad {
    bounds: [f32; 4],
    coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
}

/// Renderer-owned mesh data derived from one complete live presentation pass.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderPlan {
    mesh: UiMeshPlan,
    texture_assets: UiTextureAssetPlan,
}

impl UiRenderPlan {
    /// Flattens stock packet order into shared geometry and material runs.
    ///
    /// Source-less `SetTexture(r, g, b, a)` regions multiply that color into
    /// each live corner before selecting the renderer's vertex-color pipeline.
    /// Texture-backed regions retain their canonical MPQ path and exact tiling,
    /// blend, and residency state.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError`] when live positions or mesh counts cannot be
    /// represented by the renderer's fixed 32-bit UI ABI.
    pub fn prepare(
        presentation: &UiPresentationPlan,
        logical_extent: (f64, f64),
    ) -> Result<Self, UiRenderError> {
        let quads = presentation.members_in_draw_order().iter().map(render_quad);
        let mesh = UiMeshPlan::prepare([logical_extent.0 as f32, logical_extent.1 as f32], quads)?;
        let texture_assets = UiTextureAssetPlan::prepare(&mesh)?;
        Ok(Self {
            mesh,
            texture_assets,
        })
    }

    /// Interleaves live textures and clipped glyphs by the shared stock packet key.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError`] under the same fixed mesh ABI constraints as
    /// [`Self::prepare`].
    pub fn prepare_with_glyphs(
        presentation: &UiPresentationPlan,
        glyphs: &UiGlyphAtlasPlan,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: &UiScrollFramePlan,
        logical_extent: (f64, f64),
    ) -> Result<Self, UiRenderError> {
        let mut ordered = presentation
            .members_in_draw_order()
            .iter()
            .enumerate()
            .filter_map(|(sequence, texture)| {
                let quad = render_quad_with_scroll(texture, geometry, scroll_frames)?;
                Some((Some(texture.key()), texture.object_index(), sequence, quad))
            })
            .collect::<Vec<_>>();
        let texture_count = ordered.len();
        ordered.extend(
            glyphs
                .quads_with_scroll(geometry, scroll_frames)
                .into_iter()
                .enumerate()
                .map(|(sequence, quad)| {
                    (
                        quad.packet_key(),
                        quad.object_index(),
                        texture_count + sequence,
                        render_glyph_quad(glyphs.identity(), quad),
                    )
                }),
        );
        ordered.sort_by_key(|(key, object_index, sequence, _)| {
            (key.is_none(), *key, *object_index, *sequence)
        });
        let mesh = UiMeshPlan::prepare(
            [logical_extent.0 as f32, logical_extent.1 as f32],
            ordered.into_iter().map(|(_, _, _, quad)| quad),
        )?;
        let texture_assets = UiTextureAssetPlan::prepare(&mesh)?;
        Ok(Self {
            mesh,
            texture_assets,
        })
    }

    /// Returns upload-ready vertices, indices, and adjacent material batches.
    #[must_use]
    pub const fn mesh(&self) -> &UiMeshPlan {
        &self.mesh
    }

    /// Returns unique MPQ image requests and their batch associations.
    #[must_use]
    pub const fn texture_assets(&self) -> &UiTextureAssetPlan {
        &self.texture_assets
    }
}

/// Converts one already clipped coverage glyph into a sampled UI quad.
fn render_glyph_quad(atlas_identity: u64, glyph: UiGlyphQuad) -> UiRenderQuad {
    let color = glyph.color();
    UiRenderQuad::new(
        glyph.object_index(),
        UiRenderSource::GlyphAtlas(atlas_identity),
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        glyph.bounds(),
        glyph.texture_coordinates(),
        [color; 4],
    )
}

/// Converts one live texture region without disturbing its established order.
fn render_quad(texture: &UiTexturePresentation) -> UiRenderQuad {
    let bounds = texture.bounds();
    render_quad_parts(
        texture,
        [
            bounds.left() as f32,
            bounds.bottom() as f32,
            bounds.right() as f32,
            bounds.top() as f32,
        ],
        texture_coordinates(texture.tex_coords()),
        texture.vertex_colors(),
    )
}

/// Applies the assigned ScrollFrame translation and viewport to one texture.
fn render_quad_with_scroll(
    texture: &UiTexturePresentation,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: &UiScrollFramePlan,
) -> Option<UiRenderQuad> {
    let Some(clip_object) = texture.clip_object() else {
        return Some(render_quad(texture));
    };
    let scroll = scroll_frames.state(clip_object)?;
    let child = scroll.child()?;
    let child_scale = geometry.region(child)?.effective_scale() as f32;
    let viewport = geometry.region(clip_object)?.presentation_bounds();
    let (horizontal, vertical) = scroll.offset();
    let bounds = texture.bounds();
    let shifted = [
        bounds.left() as f32 - horizontal as f32 * child_scale,
        bounds.bottom() as f32 + vertical as f32 * child_scale,
        bounds.right() as f32 - horizontal as f32 * child_scale,
        bounds.top() as f32 + vertical as f32 * child_scale,
    ];
    let coordinates = texture_coordinates(texture.tex_coords());
    let clipped = clip_textured_quad(
        shifted,
        coordinates,
        texture.vertex_colors(),
        [
            viewport.left() as f32,
            viewport.bottom() as f32,
            viewport.right() as f32,
            viewport.top() as f32,
        ],
    )?;
    Some(render_quad_parts(
        texture,
        clipped.bounds,
        clipped.coordinates,
        clipped.colors,
    ))
}

/// Preserves one presentation's material policy around adjusted quad fields.
fn render_quad_parts(
    texture: &UiTexturePresentation,
    bounds: [f32; 4],
    coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
) -> UiRenderQuad {
    let (source, solid_color) = match texture.source() {
        UiTextureSource::Asset(path) => (UiRenderSource::Texture(path.clone()), None),
        UiTextureSource::SolidColor(color) => (UiRenderSource::VertexColor, Some(*color)),
    };
    let colors = colors.map(|color| {
        solid_color.map_or(color, |solid| {
            [
                color[0] * solid[0],
                color[1] * solid[1],
                color[2] * solid[2],
                color[3] * solid[3],
            ]
        })
    });
    UiRenderQuad::new(
        texture.object_index(),
        source,
        match texture.blend_mode() {
            UiBlendMode::Blend => UiRenderBlend::Alpha,
            UiBlendMode::Add => UiRenderBlend::Additive,
        },
        address_mode(texture.horizontal_tiling()),
        address_mode(texture.vertical_tiling()),
        if texture.non_blocking() {
            UiTextureResidency::NonBlocking
        } else {
            UiTextureResidency::Blocking
        },
        texture.desaturated(),
        bounds,
        coordinates,
        colors,
    )
}

/// Groups the flattened stock corner order for interpolation and upload.
fn texture_coordinates(coordinates: [f32; 8]) -> [[f32; 2]; 4] {
    [
        [coordinates[0], coordinates[1]],
        [coordinates[2], coordinates[3]],
        [coordinates[4], coordinates[5]],
        [coordinates[6], coordinates[7]],
    ]
}

/// Crops one translated quad and all interpolated attributes to a viewport.
fn clip_textured_quad(
    bounds: [f32; 4],
    coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
    viewport: [f32; 4],
) -> Option<ClippedTexturedQuad> {
    let [left, bottom, right, top] = bounds;
    let clipped = [
        left.max(viewport[0]),
        bottom.max(viewport[1]),
        right.min(viewport[2]),
        top.min(viewport[3]),
    ];
    if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
        return None;
    }
    let width = right - left;
    let height = top - bottom;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let left_fraction = (clipped[0] - left) / width;
    let right_fraction = (clipped[2] - left) / width;
    let top_fraction = (top - clipped[3]) / height;
    let bottom_fraction = (top - clipped[1]) / height;
    let samples = [
        [left_fraction, top_fraction],
        [left_fraction, bottom_fraction],
        [right_fraction, top_fraction],
        [right_fraction, bottom_fraction],
    ];
    Some(ClippedTexturedQuad {
        bounds: clipped,
        coordinates: samples.map(|[x, y]| interpolate_quad(coordinates, x, y)),
        colors: samples.map(|[x, y]| interpolate_quad(colors, x, y)),
    })
}

/// Bilinearly samples attributes stored as upper-left through lower-right.
fn interpolate_quad<const N: usize>(corners: [[f32; N]; 4], x: f32, y: f32) -> [f32; N] {
    std::array::from_fn(|component| {
        let top = corners[0][component] + (corners[2][component] - corners[0][component]) * x;
        let bottom = corners[1][component] + (corners[3][component] - corners[1][component]) * x;
        top + (bottom - top) * y
    })
}

/// Maps each independently authored tiling flag onto Vulkan sampler vocabulary.
const fn address_mode(is_tiled: bool) -> UiTextureAddressMode {
    if is_tiled {
        UiTextureAddressMode::Repeat
    } else {
        UiTextureAddressMode::Clamp
    }
}
