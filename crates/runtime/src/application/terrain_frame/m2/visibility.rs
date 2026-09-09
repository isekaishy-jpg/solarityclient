//! Compact immutable world bounds, separate from large animated instances.

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, placement_bounding_sphere};

/// Indexed in placement order so parent/child animation and draw order survive
/// culling. Only terrain-owned placements have immutable world transforms.
#[derive(Default)]
pub(super) struct M2PlacementVisibility {
    bounds: Vec<Option<(glam::Vec3, f32)>>,
    scenery: Vec<Option<super::distance::SceneryDistance>>,
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
        self.scenery.clear();
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
            self.scenery.push(if bounds.is_some() {
                sources[placement.source_index].as_ref().map(|source| {
                    let bounds = source.model.bounds();
                    super::distance::SceneryDistance::new(
                        bounds.minimum(),
                        bounds.maximum(),
                        placement.transform,
                    )
                })
            } else {
                None
            });
        }
    }

    pub(super) fn bounds(&self) -> &[Option<(glam::Vec3, f32)>] {
        &self.bounds
    }

    /// Static scenery alone follows the recovered CMapObj distance policy.
    pub(super) fn opacity(&self, index: usize, camera: glam::Vec3, detail: f32) -> f32 {
        self.scenery[index].map_or(1.0, |scenery| scenery.opacity(camera, detail))
    }

    pub(super) fn dynamic_indices(&self) -> &[usize] {
        &self.dynamic_indices
    }

    pub(super) fn light_parent(&self, index: usize) -> Option<usize> {
        self.light_parents[index]
    }
}
