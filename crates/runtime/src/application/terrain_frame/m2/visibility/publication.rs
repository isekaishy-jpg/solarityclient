//! Residency publication reuses static facts through the storage lineage.

use super::{M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, M2PlacementVisibility};
use crate::application::m2_spatial::StaticM2Spatial;
use crate::application::terrain_frame::m2::placements::M2PlacementStorage;
use crate::application::terrain_frame::shadow::ModelShadowKind;

/// These facts change only when a static placement is constructed or removed.
/// Source compaction relocates a slot without replacing its authored resource.
#[derive(Clone, Copy)]
pub(super) struct StaticMetadata {
    pub source_index: usize,
    spatial: Option<StaticM2Spatial>,
    distance_sort: bool,
    has_lights: bool,
    doodad: Option<super::doodads::Owner>,
    shadow_kind: Option<ModelShadowKind>,
}

impl StaticMetadata {
    /// Calculates stock spatial admission once per scenery lifetime, including
    /// the source-less placements that intentionally have no geometry.
    fn new(placement: &M2GpuPlacement, sources: &[Option<M2GpuSource>]) -> Self {
        let source = sources[placement.source_index].as_ref();
        let spatial = source.map(|source| {
            let prepare = || {
                let bounds = source.model.bounds();
                StaticM2Spatial::new(
                    bounds.minimum(),
                    bounds.maximum(),
                    bounds.sphere_radius(),
                    placement.transform,
                )
            };
            if let Some(spatial) = placement.static_spatial {
                debug_assert_eq!(spatial, prepare());
                spatial
            } else {
                prepare()
            }
        });
        Self {
            source_index: placement.source_index,
            spatial,
            distance_sort: source.is_some_and(|source| source.model.skin_profile_count() >= 2),
            has_lights: source.is_some_and(|source| !source.model.animations().lights().is_empty()),
            doodad: super::super::doodad_scene::owner_key(placement.owner),
            shadow_kind: source
                .filter(|_| {
                    placement.placement_valid
                        && placement
                            .entity_opacity
                            .as_ref()
                            .is_none_or(|owner| !owner.hidden())
                })
                .map(|source| {
                    if source.animated_shadow_caster {
                        ModelShadowKind::AnimatedScenery
                    } else {
                        ModelShadowKind::StaticScenery
                    }
                }),
        }
    }
}

impl M2PlacementVisibility {
    /// Static residency has no mutable entity-opacity/vehicle ancestry. All
    /// dynamic and retired owners retain the ordered admission path instead.
    pub(in super::super) fn static_admission_input(
        &self,
        index: usize,
        shadows: Option<crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>>,
    ) -> Option<super::super::preparation::spatial::StaticAdmissionInput> {
        let metadata = self.static_metadata.get(index).copied().flatten()?;
        Some(super::super::preparation::spatial::StaticAdmissionInput {
            spatial: metadata.spatial,
            shadow_kind: metadata.shadow_kind,
            shadow_membership: metadata.doodad.map_or(15, |owner| {
                shadows
                    .and_then(|queries| queries.doodads.get(&owner).copied())
                    .unwrap_or(0)
            }),
            publishes_lights: metadata.has_lights,
            doodad_active: metadata.doodad.is_some(),
            doodad_visible: true,
            doodad_opacity: 1.,
        })
    }

    /// Rebuilds dynamic ancestry while retaining immutable scenery metadata and
    /// relocating existing spatial nodes through the exact ordered index map.
    pub(in super::super) fn rebuild(
        &mut self,
        placements: &mut M2PlacementStorage,
        sources: &[Option<M2GpuSource>],
    ) {
        let mut profile = solarity_profiling::profile!("M2 placement metadata");
        self.dynamic_indices.clear();
        self.dynamic_indices.extend(
            placements
                .lineage()
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| (!slot.is_static).then_some(index)),
        );
        self.light_parents.clear();
        self.ancestry.rebuild_dynamic(
            placements.as_slice(),
            &self.dynamic_indices,
            &mut self.light_parents,
        );
        profile.mark("attachment parents");
        self.bounds.clear();
        self.scenery.clear();
        self.retired_indices.clear();
        self.dynamic_owners.clear();
        self.game_object_indices.clear();
        self.vehicle_parents.clear();
        self.model_distance_sort.clear();
        self.has_lights.clear();
        self.is_world_model_doodad.clear();
        self.doodads.begin();
        self.effect_start = placements.len();
        self.pending_static.clear();
        self.placement_remap.clear();
        self.placement_remap
            .resize(self.static_metadata.len(), usize::MAX);
        let mut new_static = 0;
        for (index, slot) in placements.lineage().iter().enumerate() {
            if let Some(previous) = slot.previous {
                self.placement_remap[previous] = index;
            }
            if slot.is_static {
                let metadata = slot
                    .previous
                    .and_then(|previous| self.static_metadata[previous])
                    .unwrap_or_else(|| {
                        new_static += 1;
                        StaticMetadata::new(&placements[index], sources)
                    });
                self.bounds
                    .push(metadata.spatial.map(|spatial| spatial.sphere()));
                self.scenery
                    .push(metadata.spatial.map(|spatial| spatial.scenery()));
                self.model_distance_sort.push(metadata.distance_sort);
                self.has_lights.push(metadata.has_lights);
                self.is_world_model_doodad.push(metadata.doodad.is_some());
                if let Some(owner) = metadata.doodad {
                    self.doodads.record(owner, index, metadata.has_lights);
                }
                self.pending_static.push(Some(metadata));
                continue;
            }
            self.pending_static.push(None);
            let placement = &placements[index];
            let source = sources[placement.source_index].as_ref();
            let parent = self.light_parents[index];
            let has_lights =
                source.is_some_and(|source| !source.model.animations().lights().is_empty());
            self.bounds.push(None);
            self.scenery.push(None);
            self.model_distance_sort.push(
                source.is_some_and(|source| source.model.skin_profile_count() >= 2)
                    && parent.is_none_or(|parent| self.model_distance_sort[parent]),
            );
            self.has_lights.push(has_lights);
            let doodad = super::super::doodad_scene::owner_key(placement.owner);
            self.is_world_model_doodad.push(doodad.is_some());
            if let Some(owner) = doodad {
                self.doodads.record(owner, index, has_lights);
            }
            self.dynamic_owners.entry(placement.owner).or_insert(index);
            if placement.retirement.is_some() {
                self.retired_indices.push(index);
            }
            if slot.is_effect {
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
        solarity_profiling::profile_event_value!("m2.topology.new_static_metadata", new_static);
        profile.mark("placement fields");
        self.doodads.finish();
        profile.mark("doodad membership");
        self.rebuild_scene_order();
        profile.mark("callback order");
        self.frame_work_index.remap_static(&self.placement_remap);
        self.frame_work_index
            .rebuild(&self.scenery, &self.has_lights);
        profile.mark("spatial membership");
        std::mem::swap(&mut self.static_metadata, &mut self.pending_static);
        placements.published_from(0);
    }
}
