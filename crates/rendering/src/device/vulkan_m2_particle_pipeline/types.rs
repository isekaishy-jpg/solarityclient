//! Stable particle pipeline identity and immutable material diagnostics.

use crate::M2MaterialState;

/// Stable renderer-local handle to one Vulkan particle graphics pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2ParticlePipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Fixed material state represented by one live particle pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2ParticlePipelineInfo {
    material: M2MaterialState,
}

impl M2ParticlePipelineInfo {
    pub(super) const fn new(material: M2MaterialState) -> Self {
        Self { material }
    }

    /// Returns the exact synthesized particle material in this pipeline.
    #[must_use]
    pub const fn material(self) -> M2MaterialState {
        self.material
    }
}
