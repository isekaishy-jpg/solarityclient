//! Fixture access to ordered storage without enlarging the production facade.

use super::{M2GpuPlacement, M2PlacementStorage};

impl M2PlacementStorage {
    pub(in crate::application::terrain_frame::m2) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Fixture teardown discards both simulation and its publication identity.
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        self.entries.clear();
        self.lineage.clear();
    }

    /// Explicit single-owner fixture removal preserves survivor lineage.
    pub(in crate::application::terrain_frame::m2) fn remove(
        &mut self,
        index: usize,
    ) -> M2GpuPlacement {
        self.lineage.remove(index);
        self.entries.remove(index)
    }

    pub(in crate::application::terrain_frame::m2) fn last(&self) -> Option<&M2GpuPlacement> {
        self.entries.last()
    }

    pub(in crate::application::terrain_frame::m2) fn last_mut(
        &mut self,
    ) -> Option<&mut M2GpuPlacement> {
        self.entries.last_mut()
    }
}
