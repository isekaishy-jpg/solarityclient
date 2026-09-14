//! Compact placement admission and ordering metadata, separate from animated instances.

mod doodads;
mod effects;

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource};
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
    frame_work_index: super::frame_work::M2FrameWorkIndex,
    /// Source liveness can be marked without revisiting large instance records.
    source_indices: Vec<usize>,
    bounds: Vec<Option<(glam::Vec3, f32)>>,
    scenery: Vec<Option<super::distance::SceneryDistance>>,
    dynamic_indices: Vec<usize>,
    /// Callback traversal must visit a resident attachment parent first.
    dynamic_scene_indices: Vec<usize>,
    retired_indices: Vec<usize>,
    /// First occurrence preserves the ordered lookup used by unit state updates.
    dynamic_owners: HashMap<M2GpuPlacementOwner, usize>,
    game_object_indices: Vec<usize>,
    light_parents: Vec<Option<usize>>,
    ancestry: super::ancestry::PlacementAncestry,
    vehicle_parents: HashMap<usize, usize>,
    model_distance_sort: Vec<bool>,
    has_lights: Vec<bool>,
    /// Admission must not load the large simulation record for rejected scenery.
    is_world_model_doodad: Vec<bool>,
    /// Ordered static membership survives unrelated unit/effect topology changes.
    doodads: doodads::DoodadLookup,
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
        let mut profile = solarity_profiling::profile!("M2 placement metadata");
        self.source_indices.clear();
        self.bounds.clear();
        self.scenery.clear();
        self.dynamic_indices.clear();
        self.retired_indices.clear();
        self.dynamic_owners.clear();
        self.game_object_indices.clear();
        self.ancestry.rebuild(placements, &mut self.light_parents);
        profile.mark("attachment parents");
        self.vehicle_parents.clear();
        self.model_distance_sort.clear();
        self.has_lights.clear();
        self.is_world_model_doodad.clear();
        self.doodads.begin();
        self.effect_start = placements.len();
        for (index, placement) in placements.iter().enumerate() {
            if placement.retirement.is_some() {
                self.retired_indices.push(index);
            }
            let doodad_owner = super::doodad_scene::owner_key(placement.owner);
            self.is_world_model_doodad.push(doodad_owner.is_some());
            self.source_indices.push(placement.source_index);
            let parent = self.light_parents[index];
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
            if let Some(owner) = doodad_owner {
                self.doodads.record(owner, index, has_lights);
            }
            if placement.unit_effect.is_some() {
                self.effect_start = self.effect_start.min(index);
            }
            let spatial = if matches!(placement.owner, M2GpuPlacementOwner::Static(_)) {
                source.map(|source| {
                    let prepare = || {
                        let bounds = source.model.bounds();
                        crate::application::m2_spatial::StaticM2Spatial::new(
                            bounds.minimum(),
                            bounds.maximum(),
                            bounds.sphere_radius(),
                            placement.transform,
                        )
                    };
                    if let Some(spatial) = placement.static_spatial {
                        // Static source remapping cannot change the model or transform.
                        debug_assert_eq!(spatial, prepare());
                        spatial
                    } else {
                        // Synthetic/static conversions without worker metadata still
                        // use the same complete spatial calculation.
                        prepare()
                    }
                })
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
            self.bounds.push(spatial.map(|spatial| spatial.sphere()));
            self.scenery.push(spatial.map(|spatial| spatial.scenery()));
        }
        profile.mark("placement fields");
        self.doodads.finish();
        profile.mark("doodad membership");
        self.rebuild_scene_order();
        profile.mark("callback order");
        self.frame_work_index
            .rebuild(&self.scenery, &self.has_lights);
        profile.mark("spatial membership");
    }

    /// Residency does not imply frame work. Every model packet consumer uses
    /// the same ordered query, including offscreen shadows and update owners.
    pub(super) fn select_frame_work(
        &self,
        camera: glam::Vec3,
        detail: f32,
        shadows: bool,
        work: &mut super::frame_work::M2FrameWork,
    ) {
        self.frame_work_index.select(camera, detail, shadows, work);
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
        self.doodads.index()
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

    pub(super) fn dynamic_scene_indices(&self) -> &[usize] {
        &self.dynamic_scene_indices
    }

    pub(super) fn retired_indices(&self) -> PlacementStateIndices<'_> {
        PlacementStateIndices::Cached(self.retired_indices.iter().copied())
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
        self.vehicle_parents
            .get(&index)
            .copied()
            .or(self.light_parents[index])
    }

    pub(super) fn set_vehicle_parents(&mut self, parents: &HashMap<usize, usize>) {
        if self.vehicle_parents != *parents {
            self.vehicle_parents.clone_from(parents);
            self.rebuild_scene_order();
        }
    }

    fn rebuild_scene_order(&mut self) {
        self.dynamic_scene_indices.clear();
        // Allocate by callback membership, independent of the scenery count.
        let mut first_child = HashMap::with_capacity(self.dynamic_indices.len());
        let mut next_sibling = HashMap::with_capacity(self.dynamic_indices.len());
        let mut pending = Vec::new();
        for &index in self.dynamic_indices.iter().rev() {
            if let Some(parent) = self.light_parent(index) {
                if let Some(sibling) = first_child.insert(parent, index) {
                    next_sibling.insert(index, sibling);
                }
            } else {
                pending.push(index);
            }
        }
        // 832450 recursively completes each child subtree before the scene
        // visits another root. A cycle has no root and is not admitted here.
        while let Some(index) = pending.pop() {
            self.dynamic_scene_indices.push(index);
            if let Some(&sibling) = next_sibling.get(&index) {
                pending.push(sibling);
            }
            if let Some(&child) = first_child.get(&index) {
                pending.push(child);
            }
        }
    }

    /// Vehicle ancestry is independent of scene insertion order. Invalid cycles
    /// cannot admit a shadow hierarchy or traverse indefinitely.
    pub(super) fn light_root(&self, mut index: usize) -> Option<usize> {
        for _ in 0..self.light_parents.len() {
            match self.light_parent(index) {
                Some(parent) => index = parent,
                None => return Some(index),
            }
        }
        None
    }

    pub(super) fn model_distance_sort(&self, index: usize) -> bool {
        if self.vehicle_parents.is_empty() {
            return self.model_distance_sort[index];
        }
        let mut index = index;
        for _ in 0..self.light_parents.len() {
            if !self.model_distance_sort[index] {
                return false;
            }
            match self.light_parent(index) {
                Some(parent) => index = parent,
                None => return true,
            }
        }
        false
    }

    /// Offscreen light owners still require their ordinary animation/light update.
    pub(super) fn has_lights(&self, index: usize) -> bool {
        self.has_lights[index]
    }

    pub(super) fn is_world_model_doodad(&self, index: usize) -> bool {
        self.is_world_model_doodad[index]
    }

    pub(super) fn world_model_doodad_light_indices(&self) -> &[usize] {
        self.doodads.light_indices()
    }

    pub(super) fn effect_start(&self) -> usize {
        self.effect_start
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/placement_visibility.rs"]
mod tests;
