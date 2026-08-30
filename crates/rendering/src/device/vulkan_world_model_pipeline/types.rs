//! Public pipeline identity and immutable stock-effect diagnostics.

use crate::{WorldModelMaterialState, WorldModelSpirvKey};

/// Stable renderer-local handle to one Vulkan WMO graphics pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelPipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Effect-table and fixed-function state represented by one pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelPipelineInfo {
    effect: WorldModelSpirvKey,
    material: WorldModelMaterialState,
}

impl WorldModelPipelineInfo {
    pub(super) const fn new(effect: WorldModelSpirvKey, material: WorldModelMaterialState) -> Self {
        Self { effect, material }
    }

    /// Returns the ordinary or unified MapObj shader-module identity.
    #[must_use]
    pub const fn effect(self) -> WorldModelSpirvKey {
        self.effect
    }

    /// Returns exact blend, raster, depth, and texture-count state.
    #[must_use]
    pub const fn material(self) -> WorldModelMaterialState {
        self.material
    }
}
