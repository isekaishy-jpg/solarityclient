//! Compact immutable world bounds, separate from large animated instances.

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, placement_bounding_sphere};

/// Indexed in placement order so parent/child animation and draw order survive
/// culling. Only terrain-owned placements have immutable world transforms.
#[derive(Default)]
pub(super) struct M2PlacementVisibility {
    bounds: Vec<Option<(glam::Vec3, f32)>>,
    dynamic_indices: Vec<usize>,
    light_parents: Vec<Option<usize>>,
}

impl M2PlacementVisibility {
    pub(super) fn rebuild(
        &mut self,
        placements: &[M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
    ) {
        self.bounds.clear();
        self.dynamic_indices.clear();
        self.light_parents.clear();
        for (index, placement) in placements.iter().enumerate() {
            self.light_parents
                .push(super::placement_parent_index(placements, index, placement));
            let bounds = if matches!(placement.owner, M2GpuPlacementOwner::Static(_)) {
                sources[placement.source_index]
                    .as_ref()
                    .map(|source| placement_bounding_sphere(&source.model, placement.transform))
            } else {
                self.dynamic_indices.push(index);
                None
            };
            self.bounds.push(bounds);
        }
    }

    pub(super) fn bounds(&self) -> &[Option<(glam::Vec3, f32)>] {
        &self.bounds
    }

    pub(super) fn dynamic_indices(&self) -> &[usize] {
        &self.dynamic_indices
    }

    pub(super) fn light_parent(&self, index: usize) -> Option<usize> {
        self.light_parents[index]
    }
}
