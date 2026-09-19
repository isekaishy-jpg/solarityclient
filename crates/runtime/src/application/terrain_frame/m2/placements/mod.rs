//! Ordered simulation storage with a compact publication lineage.
//!
//! Moving or retiring a dynamic placement must not erase the identity of every
//! unchanged scenery record. Structural operations carry the previous published
//! index alongside each record; animation only borrows the simulation slice.

mod access;
mod mutation;

#[cfg(test)]
#[path = "../../../../../tests/application/placement_storage.rs"]
mod tests;

use super::{M2GpuPlacement, M2GpuPlacementOwner};

/// Static owners never change model, transform or attachment family in place.
/// New residency constructs a new record; source-slot remapping is independent.
#[derive(Clone, Copy)]
pub(super) struct PlacementLineage {
    pub previous: Option<usize>,
    source_index: usize,
    pub is_static: bool,
    pub is_effect: bool,
}

impl PlacementLineage {
    /// Records immutable membership once, when simulation ownership is admitted.
    fn new(placement: &M2GpuPlacement) -> Self {
        Self {
            previous: None,
            source_index: placement.source_index,
            is_static: matches!(placement.owner, M2GpuPlacementOwner::Static(_)),
            is_effect: placement.unit_effect.is_some(),
        }
    }
}

/// Keeps publication identities aligned through ordered removal and insertion.
/// Slice access cannot change membership; only the structural methods can do so.
pub(super) struct M2PlacementStorage {
    entries: Vec<M2GpuPlacement>,
    lineage: Vec<PlacementLineage>,
    dynamic_indices: Vec<usize>,
    /// Sticky until complete publication; ordinary dynamic changes cannot reset it.
    static_layout_dirty: bool,
}

impl Default for M2PlacementStorage {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            lineage: Vec::new(),
            dynamic_indices: Vec::new(),
            static_layout_dirty: true,
        }
    }
}

impl M2PlacementStorage {
    pub(super) fn lineage(&self) -> &[PlacementLineage] {
        &self.lineage
    }

    /// Structural operations maintain this list alongside the records they move.
    pub(super) fn dynamic_indices(&self) -> &[usize] {
        &self.dynamic_indices
    }

    /// No static owner was added, removed or relocated since complete publication.
    pub(super) fn static_layout_unchanged(&self) -> bool {
        !self.static_layout_dirty
    }

    /// A dynamic-only publication leaves every static lineage identity untouched.
    pub(super) fn published_dynamic(&mut self) {
        debug_assert!(!self.static_layout_dirty);
        for &index in &self.dynamic_indices {
            self.lineage[index].previous = Some(index);
        }
    }

    /// Current references remain valid even while publication indices are dirty.
    /// Source liveness reads compact records and never opens animated instances.
    pub(super) fn mark_source_references(&self, remap: &mut [usize]) {
        for slot in &self.lineage {
            remap[slot.source_index] = 0;
        }
    }

    /// Only relocated slots write into simulation records. Removing an unused
    /// suffix leaves every existing instance untouched, including its playback.
    pub(super) fn remap_sources(&mut self, remap: &[usize]) {
        for (index, slot) in self.lineage.iter_mut().enumerate() {
            let next = remap[slot.source_index];
            if next != slot.source_index {
                slot.source_index = next;
                self.entries[index].source_index = next;
            }
        }
    }

    /// Accepts a published suffix. Effect-only publication leaves the ordinary
    /// prefix intact and must not walk or reset its scenery identities.
    pub(super) fn published_from(&mut self, first: usize) {
        for (index, lineage) in self.lineage.iter_mut().enumerate().skip(first) {
            lineage.previous = Some(index);
        }
        if first == 0 {
            self.static_layout_dirty = false;
        }
    }
}

impl From<Vec<M2GpuPlacement>> for M2PlacementStorage {
    fn from(entries: Vec<M2GpuPlacement>) -> Self {
        let lineage = entries
            .iter()
            .map(PlacementLineage::new)
            .collect::<Vec<_>>();
        let dynamic_indices = lineage
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| (!slot.is_static).then_some(index))
            .collect();
        let static_layout_dirty = true;
        Self {
            entries,
            lineage,
            dynamic_indices,
            static_layout_dirty,
        }
    }
}
