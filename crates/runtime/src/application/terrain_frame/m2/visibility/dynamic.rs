//! Dynamic topology publication keeps every unchanged static array slot in place.

use super::{M2GpuPlacementOwner, M2GpuSource, M2PlacementVisibility};
use crate::application::terrain_frame::m2::placements::M2PlacementStorage;

impl M2PlacementVisibility {
    /// Storage has proved that no static identity/index changed. Recompute the
    /// native dynamic ancestry and owner-dependent flags without a scenery scan,
    /// spatial remap or static WMO-membership reconstruction.
    pub(super) fn rebuild_dynamic(
        &mut self,
        placements: &mut M2PlacementStorage,
        sources: &[Option<M2GpuSource>],
    ) {
        let mut profile = solarity_profiling::profile!("M2 dynamic placement metadata");
        let count = placements.len();
        let indices = placements.dynamic_indices();
        solarity_profiling::profile_event_value!("m2.topology.rebuilt_placements", indices.len());
        solarity_profiling::profile_event_value!(
            "m2.topology.retained_static_placements",
            count - indices.len()
        );
        self.frame_work_index
            .replace_dynamic(&self.dynamic_indices, indices);
        self.dynamic_indices.clear();
        self.dynamic_indices.extend_from_slice(indices);
        self.ancestry
            .rebuild_dynamic(placements.as_slice(), indices, &mut self.light_parents);
        profile.mark("attachment parents");
        // Resizing preserves all static slots; any surviving non-static slot is
        // overwritten below. Removed slots can only belong to the dynamic suffix.
        self.bounds.resize(count, None);
        self.scenery.resize(count, None);
        self.static_metadata.resize(count, None);
        self.model_distance_sort.resize(count, false);
        self.has_lights.resize(count, false);
        self.is_world_model_doodad.resize(count, false);
        self.retired_indices.clear();
        self.dynamic_owners.clear();
        self.game_object_indices.clear();
        self.vehicle_parents.clear();
        self.doodads.begin();
        self.effect_start = count;
        for &index in indices {
            let placement = &placements[index];
            let source = sources[placement.source_index].as_ref();
            let parent = self.light_parents[index];
            self.model_distance_sort[index] = source
                .is_some_and(|source| source.model.skin_profile_count() >= 2)
                && parent.is_none_or(|parent| self.model_distance_sort[parent]);
            let has_lights =
                source.is_some_and(|source| !source.model.animations().lights().is_empty());
            self.has_lights[index] = has_lights;
            let doodad = super::super::doodad_scene::owner_key(placement.owner);
            self.is_world_model_doodad[index] = doodad.is_some();
            if let Some(owner) = doodad {
                self.doodads.record(owner, index, has_lights);
            }
            self.dynamic_owners.entry(placement.owner).or_insert(index);
            if placement.retirement.is_some() {
                self.retired_indices.push(index);
            }
            if placements.lineage()[index].is_effect {
                self.effect_start = self.effect_start.min(index);
            }
            if matches!(
                placement.owner,
                M2GpuPlacementOwner::GameObject { .. }
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            ) {
                self.game_object_indices.push(index);
            }
        }
        profile.mark("placement fields");
        self.doodads.finish_dynamic();
        profile.mark("doodad membership");
        self.rebuild_scene_order();
        profile.mark("callback order");
        placements.published_dynamic();
    }
}
