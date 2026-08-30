//! Public terrain pipeline identity and immutable diagnostics.

use crate::TerrainLayerCount;

/// Stable index into one renderer's terrain pipeline registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerrainPipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable shader identity retained beside one driver pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainPipelineInfo {
    layer_count: TerrainLayerCount,
}

impl TerrainPipelineInfo {
    pub(super) const fn new(layer_count: TerrainLayerCount) -> Self {
        Self { layer_count }
    }

    /// Returns the statically consumed diffuse sampler count.
    #[must_use]
    pub const fn layer_count(self) -> TerrainLayerCount {
        self.layer_count
    }
}
