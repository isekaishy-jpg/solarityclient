//! Full scalar publication retained as an independent correctness oracle.

use super::super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, M2PlacementVisibility};

impl M2PlacementVisibility {
    /// Benchmarks select the actual complete cached path without changing model
    /// identities or simulating a resource reload. No production switch is added.
    pub(in crate::application::terrain_frame::m2) fn require_full_publication(&mut self) {
        self.published = false;
    }

    /// Compares every published admission and ancestry field, independently of
    /// the cache backing it. Work queries include all distance/shadow consumers.
    pub(in crate::application::terrain_frame::m2) fn assert_matches_reference(
        &self,
        placements: &[M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
    ) {
        let mut reference = Self::default();
        reference.rebuild_reference(placements, sources);
        assert_eq!(self.bounds, reference.bounds);
        assert_eq!(self.scenery, reference.scenery);
        assert_eq!(self.dynamic_indices, reference.dynamic_indices);
        assert_eq!(self.dynamic_scene_indices, reference.dynamic_scene_indices);
        assert_eq!(self.dynamic_owners, reference.dynamic_owners);
        assert_eq!(self.retired_indices, reference.retired_indices);
        assert_eq!(self.game_object_indices, reference.game_object_indices);
        assert_eq!(self.light_parents, reference.light_parents);
        assert_eq!(self.model_distance_sort, reference.model_distance_sort);
        assert_eq!(self.has_lights, reference.has_lights);
        assert_eq!(self.is_world_model_doodad, reference.is_world_model_doodad);
        assert_eq!(self.world_model_doodads(), reference.world_model_doodads());
        assert_eq!(
            self.world_model_doodad_light_indices(),
            reference.world_model_doodad_light_indices()
        );
        assert_eq!(self.effect_start, reference.effect_start);
        for camera in [
            glam::Vec3::ZERO,
            glam::Vec3::new(100., 300., 10.),
            glam::Vec3::splat(10_000.),
        ] {
            for detail in [0., 1., 1.5] {
                for shadows in [false, true] {
                    let select = |visibility: &Self| {
                        let mut work =
                            crate::application::terrain_frame::m2::frame_work::M2FrameWork::default(
                            );
                        visibility.select_frame_work(camera, detail, shadows, &mut work);
                        std::iter::from_fn(|| work.next()).collect::<Vec<_>>()
                    };
                    assert_eq!(select(self), select(&reference));
                }
            }
        }
    }

    /// Rebuilds every placement-dependent flag in parent-first order. Residency
    /// replacement and effect publication invalidate these alongside the bounds.
    pub(in crate::application::terrain_frame::m2) fn rebuild_reference(
        &mut self,
        placements: &[M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
    ) {
        let mut profile = solarity_profiling::profile!("M2 placement metadata");
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
            let doodad_owner =
                crate::application::terrain_frame::m2::doodad_scene::owner_key(placement.owner);
            self.is_world_model_doodad.push(doodad_owner.is_some());
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
}
