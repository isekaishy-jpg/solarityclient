//! Stable ribbon pipeline identity and immutable material diagnostics.

use crate::M2MaterialState;

/// Stable renderer-local handle to one Vulkan ribbon graphics pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2RibbonPipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Fixed material state represented by one live ribbon pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2RibbonPipelineInfo {
    material: M2MaterialState,
}

impl M2RibbonPipelineInfo {
    pub(super) const fn new(material: M2MaterialState) -> Self {
        Self { material }
    }

    /// Returns the exact root M2 material state compiled into this pipeline.
    #[must_use]
    pub const fn material(self) -> M2MaterialState {
        self.material
    }
}
