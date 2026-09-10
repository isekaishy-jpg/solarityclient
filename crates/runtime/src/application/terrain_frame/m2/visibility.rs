//! Compact placement admission and ordering metadata, separate from animated instances.

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, placement_bounding_sphere};

/// Indexed in placement order so parent/child animation and draw order survive
/// culling. Only terrain-owned placements have immutable world transforms.
#[derive(Default)]
pub(super) struct M2PlacementVisibility {
    /// Source liveness can be marked without revisiting large instance records.
    source_indices: Vec<usize>,
    bounds: Vec<Option<(glam::Vec3, f32)>>,
    scenery: Vec<Option<super::distance::SceneryDistance>>,
    dynamic_indices: Vec<usize>,
    light_parents: Vec<Option<usize>>,
    model_distance_sort: Vec<bool>,
    has_lights: Vec<bool>,
    /// First effect, or the placement count when no effect is resident.
    effect_start: usize,
}

impl M2PlacementVisibility {
    /// Rebuilds every placement-dependent flag in parent-first order. Residency
    /// replacement and effect publication invalidate these alongside the bounds.
    pub(super) fn rebuild(
        &mut self,
        placements: &[M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
    ) {
        self.source_indices.clear();
        self.bounds.clear();
        self.scenery.clear();
        self.dynamic_indices.clear();
        self.light_parents.clear();
        self.model_distance_sort.clear();
        self.has_lights.clear();
        self.effect_start = placements.len();
        for (index, placement) in placements.iter().enumerate() {
            self.source_indices.push(placement.source_index);
            let parent = super::placement_parent_index(placements, index, placement);
            self.light_parents.push(parent);
            let source = sources[placement.source_index].as_ref();
            // Stock enables whole-model sorting for two or more external views;
            // an attachment inherits the containing model's transparency domain.
            let authored_sort = source.is_some_and(|source| source.model.skin_profile_count() >= 2);
            self.model_distance_sort.push(
                authored_sort && parent.is_none_or(|parent| self.model_distance_sort[parent]),
            );
            self.has_lights
                .push(source.is_some_and(|source| !source.model.animations().lights().is_empty()));
            if placement.unit_effect.is_some() {
                self.effect_start = self.effect_start.min(index);
            }
            let bounds = if matches!(placement.owner, M2GpuPlacementOwner::Static(_)) {
                source.map(|source| placement_bounding_sphere(&source.model, placement.transform))
            } else {
                self.dynamic_indices.push(index);
                None
            };
            self.bounds.push(bounds);
            self.scenery.push(if bounds.is_some() {
                source.map(|source| {
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

    /// Marks all placement-owned slots, including dynamic and empty geometry.
    /// Callers must rebuild after any placement or source-index remap before
    /// using this compact list; the ordinary topology dirty flag covers both.
    pub(super) fn mark_source_references(&self, remap: &mut [usize]) {
        for &source_index in &self.source_indices {
            remap[source_index] = 0;
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

    pub(super) fn model_distance_sort(&self, index: usize) -> bool {
        self.model_distance_sort[index]
    }

    /// Offscreen light owners still require their ordinary animation/light update.
    pub(super) fn has_lights(&self, index: usize) -> bool {
        self.has_lights[index]
    }

    pub(super) fn effect_start(&self) -> usize {
        self.effect_start
    }
}
