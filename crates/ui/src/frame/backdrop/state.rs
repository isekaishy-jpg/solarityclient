//! Compact backdrop state after XML inheritance has applied.

use solarity_asset::AssetPath;

use crate::{
    UiBackdropFile, UiBackdropLayer, UiBackdropPlan, UiBlendMode, UiFrameError, UiObjectTree,
};

const WHITE: [f32; 4] = [1.0; 4];

/// Final native backdrop properties for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBackdropState {
    background: Option<AssetPath>,
    edge: Option<AssetPath>,
    tiled: bool,
    tile_size: f32,
    edge_size: f32,
    insets: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    blend_mode: UiBlendMode,
}

impl UiBackdropState {
    /// Returns the canonical background texture, when declared.
    #[must_use]
    pub const fn background(&self) -> Option<&AssetPath> {
        self.background.as_ref()
    }

    /// Returns the canonical eight-piece edge atlas, when declared.
    #[must_use]
    pub const fn edge(&self) -> Option<&AssetPath> {
        self.edge.as_ref()
    }

    /// Returns whether the background repeats at `tile_size` intervals.
    #[must_use]
    pub const fn tiled(&self) -> bool {
        self.tiled
    }

    /// Returns the logical size of one background tile.
    #[must_use]
    pub const fn tile_size(&self) -> f32 {
        self.tile_size
    }

    /// Returns the logical thickness of border edges and corners.
    #[must_use]
    pub const fn edge_size(&self) -> f32 {
        self.edge_size
    }

    /// Returns left, right, top, and bottom background insets.
    #[must_use]
    pub const fn insets(&self) -> [f32; 4] {
        self.insets
    }

    /// Returns the XML background modulation color.
    #[must_use]
    pub const fn color(&self) -> [f32; 4] {
        self.color
    }

    /// Returns the XML border modulation color.
    #[must_use]
    pub const fn border_color(&self) -> [f32; 4] {
        self.border_color
    }

    /// Returns the common background and border blend mode.
    #[must_use]
    pub const fn blend_mode(&self) -> UiBlendMode {
        self.blend_mode
    }

    fn initial() -> Self {
        Self {
            background: None,
            edge: None,
            tiled: false,
            tile_size: 32.0,
            edge_size: 32.0,
            insets: [0.0; 4],
            color: WHITE,
            border_color: WHITE,
            blend_mode: UiBlendMode::Blend,
        }
    }

    fn apply(&mut self, layer: &UiBackdropLayer) {
        apply_file(&mut self.background, layer.background.as_ref());
        apply_file(&mut self.edge, layer.edge.as_ref());
        if let Some(value) = layer.tiled {
            self.tiled = value;
        }
        if let Some(value) = layer.tile_size {
            self.tile_size = value;
        }
        if let Some(value) = layer.edge_size {
            self.edge_size = value;
        }
        if let Some(value) = layer.insets {
            self.insets = value;
        }
        if let Some(value) = layer.color {
            self.color = value;
        }
        if let Some(value) = layer.border_color {
            self.border_color = value;
        }
        if let Some(value) = layer.blend_mode {
            self.blend_mode = value;
        }
    }
}

/// Compact optional backdrop states indexed by the complete object arena.
pub struct UiBackdropStatePlan {
    node_states: Vec<Option<u32>>,
    states: Vec<UiBackdropState>,
}

impl UiBackdropStatePlan {
    /// Resolves inherited backdrop declarations once per owning frame.
    ///
    /// # Errors
    ///
    /// Returns [`UiFrameError::Resolution`] when declaration and object arenas
    /// differ or a compact state index exceeds `u32`.
    pub fn resolve(tree: &UiObjectTree<'_>, plan: &UiBackdropPlan) -> Result<Self, UiFrameError> {
        let mut node_states = vec![None; tree.nodes().len()];
        let mut states = Vec::with_capacity(plan.layer_count());
        for (node_index, node_state) in node_states.iter_mut().enumerate() {
            let node = plan
                .node(node_index)
                .ok_or_else(|| UiFrameError::Resolution {
                    message: "backdrop plan and object tree have different node counts".to_owned(),
                })?;
            if node.layer_count() == 0 {
                continue;
            }
            let mut state = UiBackdropState::initial();
            for layer in plan.layers_for(node) {
                state.apply(layer);
            }
            if state.background.is_none() && state.edge.is_none() {
                continue;
            }
            let state_index =
                u32::try_from(states.len()).map_err(|error| UiFrameError::Resolution {
                    message: format!("backdrop state index exceeds u32: {error}"),
                })?;
            *node_state = Some(state_index);
            states.push(state);
        }
        Ok(Self {
            node_states,
            states,
        })
    }

    /// Returns the resolved backdrop for one object-arena index.
    #[must_use]
    pub fn state(&self, node_index: usize) -> Option<&UiBackdropState> {
        let state_index = self.node_states.get(node_index)?.as_ref()?;
        self.states.get(*state_index as usize)
    }

    /// Returns the number of frames carrying a native backdrop.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
}

fn apply_file(target: &mut Option<AssetPath>, value: Option<&UiBackdropFile>) {
    match value {
        Some(UiBackdropFile::Asset(path)) => *target = Some(path.clone()),
        Some(UiBackdropFile::Clear) => *target = None,
        None => {}
    }
}
