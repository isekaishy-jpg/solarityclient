//! Compact texture state after XML inheritance and declaration ordering.

use crate::{
    UiBlendMode, UiDrawLayer, UiGradientOrientation, UiObjectKind, UiObjectTree, UiTexCoords,
    UiTextureColor, UiTextureError, UiTextureFile, UiTextureLayer, UiTexturePlan,
};

const DEFAULT_TEX_COORDS: [f32; 8] = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Final startup properties for one textured render region.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextureState {
    file: Option<UiTextureFile>,
    solid_color: Option<[f32; 4]>,
    blend_mode: UiBlendMode,
    tex_coords: [f32; 8],
    vertex_colors: [[f32; 4]; 4],
    horizontal_tiling: bool,
    vertical_tiling: bool,
    non_blocking: bool,
    draw_layer: UiDrawLayer,
    draw_sub_level: i16,
}

impl UiTextureState {
    /// Returns the final archive asset or explicit dynamic marker.
    #[must_use]
    pub const fn file(&self) -> Option<&UiTextureFile> {
        self.file.as_ref()
    }

    /// Returns the uniform texture payload created by an XML `Color` child.
    #[must_use]
    pub const fn solid_color(&self) -> Option<[f32; 4]> {
        self.solid_color
    }

    /// Returns final source-alpha or additive blending.
    #[must_use]
    pub const fn blend_mode(&self) -> UiBlendMode {
        self.blend_mode
    }

    /// Returns upper-left, lower-left, upper-right, and lower-right UV pairs.
    #[must_use]
    pub const fn tex_coords(&self) -> [f32; 8] {
        self.tex_coords
    }

    /// Returns colors for upper-left, lower-left, upper-right, and lower-right.
    #[must_use]
    pub const fn vertex_colors(&self) -> [[f32; 4]; 4] {
        self.vertex_colors
    }

    /// Returns whether horizontal wrapping was explicitly enabled.
    #[must_use]
    pub const fn horizontal_tiling(&self) -> bool {
        self.horizontal_tiling
    }

    /// Returns whether vertical wrapping was explicitly enabled.
    #[must_use]
    pub const fn vertical_tiling(&self) -> bool {
        self.vertical_tiling
    }

    /// Returns the stock non-blocking residency request.
    #[must_use]
    pub const fn non_blocking(&self) -> bool {
        self.non_blocking
    }

    /// Returns the resolved region draw band.
    #[must_use]
    pub const fn draw_layer(&self) -> UiDrawLayer {
        self.draw_layer
    }

    /// Returns the signed ordering offset inside the draw band.
    #[must_use]
    pub const fn draw_sub_level(&self) -> i16 {
        self.draw_sub_level
    }

    fn initial() -> Self {
        Self {
            file: None,
            solid_color: None,
            blend_mode: UiBlendMode::Blend,
            tex_coords: DEFAULT_TEX_COORDS,
            vertex_colors: [WHITE; 4],
            horizontal_tiling: false,
            vertical_tiling: false,
            non_blocking: false,
            draw_layer: UiDrawLayer::Artwork,
            draw_sub_level: 0,
        }
    }

    fn apply(&mut self, layer: &UiTextureLayer) {
        if let Some(blend_mode) = layer.blend_mode() {
            self.blend_mode = blend_mode;
        }
        if let Some(coords) = layer.tex_coords() {
            apply_tex_coords(&mut self.tex_coords, coords);
        }
        if let Some(color) = layer.color() {
            // CSimpleTexture::LoadXML (0x00485F40) creates a color texture
            // before applying this declaration's file attribute. It replaces
            // an inherited source without changing inherited vertex tint.
            self.file = None;
            self.solid_color = Some(rgba(color));
        }
        if let Some(gradient) = layer.gradient() {
            match gradient.orientation() {
                UiGradientOrientation::Vertical => {
                    let minimum = rgba(gradient.minimum());
                    let maximum = rgba(gradient.maximum());
                    self.vertex_colors = [maximum, minimum, maximum, minimum];
                }
            }
        }
        match layer.file() {
            Some(file @ UiTextureFile::Asset(_)) => {
                self.file = Some(file.clone());
                self.solid_color = None;
                if let Some(color) = layer.color().map(rgba)
                    && color != [0.0; 4]
                {
                    self.vertex_colors = [color; 4];
                }
            }
            Some(UiTextureFile::Dynamic) if self.file.is_none() && self.solid_color.is_none() => {
                self.file = Some(UiTextureFile::Dynamic);
            }
            Some(UiTextureFile::Dynamic) | None => {}
        }
        if let Some(value) = layer.horizontal_tiling() {
            self.horizontal_tiling = value;
        }
        if let Some(value) = layer.vertical_tiling() {
            self.vertical_tiling = value;
        }
        if let Some(value) = layer.non_blocking() {
            self.non_blocking = value;
        }
        if let Some(value) = layer.draw_layer() {
            self.draw_layer = value;
        }
        if let Some(value) = layer.draw_sub_level() {
            self.draw_sub_level = value;
        }
    }
}

/// Compact states indirectly indexed by the complete UI object arena.
pub struct UiTextureStatePlan {
    node_states: Vec<Option<u32>>,
    states: Vec<UiTextureState>,
}

impl UiTextureStatePlan {
    /// Resolves every texture layer once in inherited application order.
    ///
    /// # Errors
    ///
    /// Returns [`UiTextureError::Resolution`] when the tree and declaration plan
    /// do not describe the same object arena or the compact index overflows.
    pub fn resolve(tree: &UiObjectTree<'_>, plan: &UiTexturePlan) -> Result<Self, UiTextureError> {
        let mut node_states = vec![None; tree.nodes().len()];
        let texture_count = tree
            .nodes()
            .iter()
            .filter(|node| node.kind() == UiObjectKind::Texture)
            .count();
        let mut states = Vec::with_capacity(texture_count);
        for (node_index, object) in tree.nodes().iter().enumerate() {
            let node = plan
                .node(node_index)
                .ok_or_else(|| UiTextureError::Resolution {
                    message: "texture plan and object tree have different node counts".to_owned(),
                })?;
            if object.kind() != UiObjectKind::Texture {
                continue;
            }
            let mut state = UiTextureState::initial();
            for layer in plan.layers_for(node) {
                state.apply(layer);
            }
            let state_index =
                u32::try_from(states.len()).map_err(|error| UiTextureError::Resolution {
                    message: format!("texture state index exceeds u32: {error}"),
                })?;
            node_states[node_index] = Some(state_index);
            states.push(state);
        }
        Ok(Self {
            node_states,
            states,
        })
    }

    /// Returns final state for one texture object index.
    #[must_use]
    pub fn state(&self, node_index: usize) -> Option<&UiTextureState> {
        let state_index = self.node_states.get(node_index)?.as_ref()?;
        self.states.get(*state_index as usize)
    }

    /// Returns the number of resolved texture objects.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
}

fn apply_tex_coords(output: &mut [f32; 8], coords: UiTexCoords) {
    let left = coords.left().unwrap_or(output[0]);
    let right = coords.right().unwrap_or(output[4]);
    let top = coords.top().unwrap_or(output[1]);
    let bottom = coords.bottom().unwrap_or(output[3]);
    *output = [left, top, left, bottom, right, top, right, bottom];
}

fn rgba(color: UiTextureColor) -> [f32; 4] {
    [
        color.red(),
        color.green(),
        color.blue(),
        color.alpha().unwrap_or(1.0),
    ]
}
