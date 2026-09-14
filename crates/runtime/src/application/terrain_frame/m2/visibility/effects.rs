//! Effect-tail publication preserves immutable scenery and ordinary membership.

use super::{M2GpuSource, M2PlacementVisibility};

impl M2PlacementVisibility {
    /// Replaces only the effect suffix after an append or ordered retirement.
    /// The ordinary prefix and source identities must be unchanged. Source-slot
    /// compaction has its own remap and does not invalidate spatial metadata.
    pub(in super::super) fn replace_effect_tail(
        &mut self,
        placements: &mut super::super::placements::M2PlacementStorage,
        sources: &[Option<M2GpuSource>],
    ) {
        let first = self.effect_start;
        debug_assert!(first <= placements.len());
        self.bounds.truncate(first);
        self.scenery.truncate(first);
        self.model_distance_sort.truncate(first);
        self.has_lights.truncate(first);
        self.is_world_model_doodad.truncate(first);
        self.dynamic_indices.retain(|index| *index < first);
        self.dynamic_owners.retain(|_, index| *index < first);
        self.retired_indices.retain(|index| *index < first);
        self.dynamic_indices.extend(first..placements.len());
        self.ancestry.rebuild_dynamic(
            placements.as_slice(),
            &self.dynamic_indices,
            &mut self.light_parents,
        );
        for (index, placement) in placements.iter().enumerate().skip(first) {
            debug_assert!(placement.unit_effect.is_some());
            let source = sources[placement.source_index].as_ref();
            self.bounds.push(None);
            self.scenery.push(None);
            self.has_lights
                .push(source.is_some_and(|source| !source.model.animations().lights().is_empty()));
            self.is_world_model_doodad.push(false);
            self.dynamic_owners.entry(placement.owner).or_insert(index);
            let parent = self.light_parents[index];
            self.model_distance_sort.push(
                source.is_some_and(|source| source.model.skin_profile_count() >= 2)
                    && parent.is_none_or(|parent| self.model_distance_sort[parent]),
            );
            if placement.retirement.is_some() {
                self.retired_indices.push(index);
            }
        }
        self.rebuild_scene_order();
        self.frame_work_index
            .replace_effect_tail(first, placements.len());
        self.static_metadata.truncate(first);
        self.static_metadata.resize(placements.len(), None);
        placements.published_from(first);
    }

    /// Resource-slot relocation does not change authored bounds or membership.
    pub(in super::super) fn remap_sources(&mut self, remap: &[usize]) {
        // Pending removals can leave dead cached slots until the next topology
        // publication. Their lineage is gone, so they must not index a later map.
        for metadata in self.static_metadata.iter_mut().flatten() {
            if metadata.source_index != usize::MAX {
                metadata.source_index = remap[metadata.source_index];
            }
        }
    }
}
