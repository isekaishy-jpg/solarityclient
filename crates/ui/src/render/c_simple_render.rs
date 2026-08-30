//! Batched UI mesh generation recovered from `CSimpleRender.cpp`.

use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiTextureAddressMode,
    UiTextureResidency,
};

use crate::{
    UiBlendMode, UiPresentationPlan, UiRenderError, UiTextureAssetPlan, UiTexturePresentation,
    UiTextureSource,
};

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

/// Converts one live texture region without disturbing its established order.
fn render_quad(texture: &UiTexturePresentation) -> UiRenderQuad {
    let (source, solid_color) = match texture.source() {
        UiTextureSource::Asset(path) => (UiRenderSource::Texture(path.clone()), None),
        UiTextureSource::SolidColor(color) => (UiRenderSource::VertexColor, Some(*color)),
    };
    let colors = texture.vertex_colors().map(|color| {
        solid_color.map_or(color, |solid| {
            [
                color[0] * solid[0],
                color[1] * solid[1],
                color[2] * solid[2],
                color[3] * solid[3],
            ]
        })
    });
    let bounds = texture.bounds();
    let coordinates = texture.tex_coords();
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
        [
            bounds.left() as f32,
            bounds.bottom() as f32,
            bounds.right() as f32,
            bounds.top() as f32,
        ],
        [
            [coordinates[0], coordinates[1]],
            [coordinates[2], coordinates[3]],
            [coordinates[4], coordinates[5]],
            [coordinates[6], coordinates[7]],
        ],
        colors,
    )
}

/// Maps each independently authored tiling flag onto Vulkan sampler vocabulary.
const fn address_mode(is_tiled: bool) -> UiTextureAddressMode {
    if is_tiled {
        UiTextureAddressMode::Repeat
    } else {
        UiTextureAddressMode::Clamp
    }
}
