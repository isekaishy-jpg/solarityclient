//! Compact placement admission and ordering metadata, separate from animated instances.

mod doodads;
mod effects;
mod publication;

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
    static_metadata: Vec<Option<publication::StaticMetadata>>,
    pending_static: Vec<Option<publication::StaticMetadata>>,
    /// Previous publication indices map to their new slot or usize::MAX on removal.
    placement_remap: Vec<usize>,
    frame_work_index: super::frame_work::M2FrameWorkIndex,
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
