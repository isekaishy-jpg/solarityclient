//! Compact placement admission and ordering metadata, separate from animated instances.

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, placement_bounding_sphere};
use std::collections::HashMap;

/// Ordered state-update candidates, borrowing rebuilt metadata or covering the
/// current records while scene publication has invalidated placement indices.
pub(super) enum PlacementStateIndices<'a> {
    Cached(std::iter::Copied<std::slice::Iter<'a, usize>>),
    All(std::ops::Range<usize>),
}

impl Iterator for PlacementStateIndices<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Cached(indices) => indices.next(),
            Self::All(indices) => indices.next(),
        }
    }
}

/// Indexed in placement order so parent/child animation and draw order survive
/// culling. Only terrain-owned placements have immutable world transforms.
#[derive(Default)]
pub(super) struct M2PlacementVisibility {
    /// Source liveness can be marked without revisiting large instance records.
    source_indices: Vec<usize>,
    bounds: Vec<Option<(glam::Vec3, f32)>>,
    scenery: Vec<Option<super::distance::SceneryDistance>>,
    dynamic_indices: Vec<usize>,
    /// First occurrence preserves the ordered lookup used by unit state updates.
    dynamic_owners: HashMap<M2GpuPlacementOwner, usize>,
    game_object_indices: Vec<usize>,
    light_parents: Vec<Option<usize>>,
    model_distance_sort: Vec<bool>,
    has_lights: Vec<bool>,
    /// Admission must not load the large simulation record for rejected scenery.
    is_world_model_doodad: Vec<bool>,
    /// Light owners are evaluated even without a visible MODR reference.
    world_model_doodad_light_indices: Vec<usize>,
    /// First effect, or the placement count when no effect is resident.
    effect_start: usize,
    world_model_doodads: HashMap<
        (
            crate::application::terrain_coordinator::RuntimeWorldModelMovementOwner,
            usize,
        ),
        usize,
    >,
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
        self.dynamic_owners.clear();
        self.game_object_indices.clear();
        self.light_parents.clear();
        self.model_distance_sort.clear();
        self.has_lights.clear();
        self.is_world_model_doodad.clear();
        self.world_model_doodad_light_indices.clear();
        self.effect_start = placements.len();
        self.world_model_doodads.clear();
        for (index, placement) in placements.iter().enumerate() {
            let doodad_owner = super::doodad_scene::owner_key(placement.owner);
            self.is_world_model_doodad.push(doodad_owner.is_some());
            let first_doodad = doodad_owner.is_some_and(|owner| {
                *self.world_model_doodads.entry(owner).or_insert(index) == index
            });
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
            let has_lights =
                source.is_some_and(|source| !source.model.animations().lights().is_empty());
            self.has_lights.push(has_lights);
            if first_doodad && has_lights {
                self.world_model_doodad_light_indices.push(index);
            }
            if placement.unit_effect.is_some() {
                self.effect_start = self.effect_start.min(index);
            }
            let bounds = if matches!(placement.owner, M2GpuPlacementOwner::Static(_)) {
                source.map(|source| placement_bounding_sphere(&source.model, placement.transform))
            } else {
                self.dynamic_indices.push(index);
                self.dynamic_owners.entry(placement.owner).or_insert(index);
                if matches!(
                    placement.owner,
                    M2GpuPlacementOwner::GameObject { .. }
                        | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                ) {
                    self.game_object_indices.push(index);
                }
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

    pub(super) fn world_model_doodads(
        &self,
    ) -> &HashMap<
        (
            crate::application::terrain_coordinator::RuntimeWorldModelMovementOwner,
            usize,
        ),
        usize,
    > {
        &self.world_model_doodads
    }

    pub(super) fn scenery(&self, index: usize) -> Option<super::distance::SceneryDistance> {
        self.scenery[index]
    }

    /// Static scenery alone follows the recovered CMapObj distance policy.
    pub(super) fn opacity(&self, index: usize, camera: glam::Vec3, detail: f32) -> f32 {
        self.scenery[index].map_or(1.0, |scenery| scenery.opacity(camera, detail))
    }

    pub(super) fn dynamic_indices(&self) -> &[usize] {
        &self.dynamic_indices
    }

    pub(super) fn dynamic_owner_index(&self, owner: M2GpuPlacementOwner) -> Option<usize> {
        self.dynamic_owners.get(&owner).copied()
    }

    /// Includes offscreen objects and doodads in their original update order.
    /// Like the owner index, this requires current placement topology metadata.
    pub(super) fn game_object_indices(&self) -> PlacementStateIndices<'_> {
        PlacementStateIndices::Cached(self.game_object_indices.iter().copied())
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

    pub(super) fn is_world_model_doodad(&self, index: usize) -> bool {
        self.is_world_model_doodad[index]
    }

    pub(super) fn world_model_doodad_light_indices(&self) -> &[usize] {
        &self.world_model_doodad_light_indices
    }

    pub(super) fn effect_start(&self) -> usize {
        self.effect_start
    }
}
