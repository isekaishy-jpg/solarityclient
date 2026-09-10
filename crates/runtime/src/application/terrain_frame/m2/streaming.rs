//! Static M2 placement ownership across overlapping resident ADTs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use solarity_asset::AnimationDataCatalog;
use solarity_rendering::{M2ModelOrientation, M2RibbonTrail, VulkanRenderer};

use crate::application::frame_profile::RuntimeFrameProfile;
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Owner, ResidentM2Placement, ResidentM2Scene,
};
use crate::random::CrtRand;

use super::{
    M2Frame, M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, M2Playback,
    RuntimeTerrainFrameError, prepare_source, stock_particle_simulations,
};

/// Compact static identities avoid scanning animation and effect state to reuse an owner.
/// Dynamic sources never enter this index because their replacement textures can differ.
#[derive(Default)]
pub(super) struct StaticM2Residency {
    owners: HashSet<ResidentM2Owner>,
    source_indices: Vec<usize>,
    // Shared scene ownership makes generation identity stable even when the
    // coordinator moves a tile between primary and neighboring residency.
    scenes: Vec<Arc<ResidentM2Scene>>,
    references: HashMap<ResidentM2Owner, usize>,
}

/// Proposed scene references remain separate until all new GPU owners are ready.
struct StaticM2SceneUpdate {
    scenes: Vec<Arc<ResidentM2Scene>>,
    added: Vec<usize>,
    // Final counts for owners touched by arriving or departing scenes only.
    references: HashMap<ResidentM2Owner, usize>,
}

impl StaticM2Residency {
    /// Seeds the index from the same scene used to create the initial static owners.
    pub(super) fn new(scene: &ResidentM2Scene) -> Self {
        let owners = scene
            .placements()
            .iter()
            .map(ResidentM2Placement::owner)
            .collect();
        let mut source_indices = scene
            .placements()
            .iter()
            .map(ResidentM2Placement::source_index)
            .collect::<Vec<_>>();
        source_indices.sort_unstable();
        source_indices.dedup();
        Self {
            owners,
            source_indices,
            // GPU preparation can precede the first full tile publication. Its
            // scene is not an extra reference: it may already have departed.
            scenes: Vec::new(),
            references: HashMap::new(),
        }
    }

    /// Counts only changed generations, preserving caller order for new owners.
    /// Duplicate references to the same scene never register it twice.
    fn stage_scenes<'a>(
        &self,
        scenes: impl Iterator<Item = &'a Arc<ResidentM2Scene>>,
    ) -> StaticM2SceneUpdate {
        let mut update = StaticM2SceneUpdate {
            scenes: Vec::new(),
            added: Vec::new(),
            references: HashMap::new(),
        };
        for scene in scenes {
            if update.scenes.iter().any(|old| Arc::ptr_eq(old, scene)) {
                continue;
            }
            if !self.scenes.iter().any(|old| Arc::ptr_eq(old, scene)) {
                update.added.push(update.scenes.len());
                for placement in scene.placements() {
                    let owner = placement.owner();
                    *update
                        .references
                        .entry(owner)
                        .or_insert_with(|| self.references.get(&owner).copied().unwrap_or(0)) += 1;
                }
            }
            update.scenes.push(Arc::clone(scene));
        }
        // Apply arrivals before departures so a shared owner can cross tile
        // generations without a transient zero count or a restarted clock.
        for scene in &self.scenes {
            if update.scenes.iter().any(|next| Arc::ptr_eq(next, scene)) {
                continue;
            }
            for placement in scene.placements() {
                let owner = placement.owner();
                *update
                    .references
                    .entry(owner)
                    .or_insert_with(|| self.references[&owner]) -= 1;
            }
        }
        update
    }

    /// Commits references and returns only owners whose last scene reference left.
    fn publish_scenes(&mut self, update: StaticM2SceneUpdate) -> HashSet<ResidentM2Owner> {
        let mut retired = HashSet::new();
        if self.scenes.is_empty() {
            // Initial GPU preparation seeds owners before any scene references
            // are published. That initial scene may already have departed, so
            // its absent owners cannot be recovered from a departure delta.
            retired.extend(
                self.owners
                    .iter()
                    .copied()
                    .filter(|owner| update.references.get(owner).copied().unwrap_or(0) == 0),
            );
        }
        for (owner, count) in update.references {
            if count == 0 {
                self.references.remove(&owner);
                retired.insert(owner);
            } else {
                self.references.insert(owner, count);
            }
        }
        self.scenes = update.scenes;
        retired
    }

    /// Follows the common static/dynamic source compactor without retaining dead slots.
    fn remap_sources(&mut self, remap: &[usize]) {
        self.source_indices.retain_mut(|index| {
            let mapped = remap[*index];
            if mapped == usize::MAX {
                return false;
            }
            *index = mapped;
            true
        });
    }
}

impl M2Frame {
    /// Retains each MDDF/MODD owner until its last resident ADT reference leaves.
    ///
    /// Shared MODF/MDDF identifiers are world placement identities in build
    /// 12340 (Map.cpp's object registration and 0x007A50C0's chunk references).
    /// New tile references must not create a second playback or emitter owner.
    pub(in crate::application::terrain_frame) fn synchronize_static_scenes<'a>(
        &mut self,
        renderer: &mut VulkanRenderer,
        scenes: impl Iterator<Item = &'a Arc<ResidentM2Scene>>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut profile = RuntimeFrameProfile::new("M2 residency publication");
        let update = self.static_residency.stage_scenes(scenes);
        profile.mark("scene reference changes");
        // Source reuse is confined to static owners: character texture
        // replacements can share an M2 model but represent different materials.
        let mut sources = HashMap::with_capacity(self.static_residency.source_indices.len());
        for &source_index in &self.static_residency.source_indices {
            if let Some(source) = self.sources[source_index].as_ref() {
                sources.insert(Arc::as_ptr(&source.model), source_index);
            }
        }
        let mut added = Vec::new();
        let mut added_owners = HashSet::new();
        profile.mark("retained owners and sources");
        for &index in &update.added {
            let scene = &update.scenes[index];
            for placement in scene.placements() {
                if self.static_residency.owners.contains(&placement.owner())
                    || !added_owners.insert(placement.owner())
                {
                    continue;
                }
                let source = scene.sources().get(placement.source_index()).ok_or(
                    RuntimeTerrainFrameError::M2SourceIndex {
                        source_index: placement.source_index(),
                        source_count: scene.sources().len(),
                    },
                )?;
                let identity = Arc::as_ptr(source.model());
                let source_index = if let Some(&index) = sources.get(&identity) {
                    index
                } else {
                    let gpu = prepare_source(renderer, source)?;
                    let index = self.sources.len();
                    self.sources.push(gpu);
                    self.static_residency.source_indices.push(index);
                    sources.insert(identity, index);
                    index
                };
                added.push(static_gpu_placement(
                    placement,
                    source_index,
                    self.sources[source_index].as_ref(),
                    &self.animations,
                    self.animation_time_ms(),
                    random,
                )?);
            }
        }
        profile.mark("new sources and placements");
        // Publish references only after every new owner is ready, so a failed
        // resource preparation cannot make the next attempt skip that owner.
        let retired = self.static_residency.publish_scenes(update);
        // Both retention paths mark source use for the later compactor. Include
        // new owners before they join the placement vector.
        let mut remap = vec![usize::MAX; self.sources.len()];
        for placement in &added {
            remap[placement.source_index] = 0;
        }
        if retired.is_empty() {
            // Most tile admissions and departures leave every shared M2 owner
            // alive. Mark source use directly; unchanged owner lifetimes need
            // no membership checks or placement record compaction.
            if self.placement_topology_dirty {
                for placement in &self.placements {
                    remap[placement.source_index] = 0;
                }
            } else {
                self.placement_visibility.mark_source_references(&mut remap);
            }
        } else {
            self.placements.retain(|placement| {
                let keep = match placement.owner {
                    M2GpuPlacementOwner::Static(owner) => {
                        let keep = !retired.contains(&owner);
                        if !keep {
                            self.static_residency.owners.remove(&owner);
                        }
                        keep
                    }
                    _ => true,
                };
                if keep {
                    remap[placement.source_index] = 0;
                }
                keep
            });
        }
        profile.mark("placement retirement");
        self.placements.extend(added);
        self.static_residency.owners.extend(added_owners);
        self.placement_topology_dirty = true;
        profile.mark("placement append and owner publication");
        self.compact_referenced_sources(remap);
        profile.mark("source compaction");
        Ok(())
    }

    /// Drops unreferenced sources and remaps surviving static and dynamic slots.
    /// Empty geometry is still an occupied source when a placement references it.
    pub(super) fn compact_sources(&mut self) {
        let mut profile = RuntimeFrameProfile::new("M2 source compaction");
        let mut remap = vec![usize::MAX; self.sources.len()];
        for placement in &self.placements {
            remap[placement.source_index] = 0;
        }
        profile.mark("referenced slots");
        self.compact_referenced_sources(remap);
        profile.mark("source compaction");
    }

    /// Consumes one mark per source: zero is referenced, `usize::MAX` is dead.
    /// Callers must mark every retained and newly added placement, including
    /// dynamic owners and sources whose geometry is intentionally empty.
    fn compact_referenced_sources(&mut self, mut remap: Vec<usize>) {
        let mut profile = RuntimeFrameProfile::new("M2 referenced source compaction");
        // A residency change often leaves every shared source referenced. Its
        // mapping is then the identity: avoid writing every large live instance.
        if !remap.contains(&usize::MAX) {
            return;
        }
        // The compact placement metadata also retains source slots. Rebuild it
        // after a remap before any later publication can mark source liveness.
        self.placement_topology_dirty = true;
        let mut index = 0;
        let mut next = 0;
        self.sources.retain(|_| {
            let keep = remap[index] != usize::MAX;
            if keep {
                remap[index] = next;
                next += 1;
            }
            index += 1;
            keep
        });
        profile.mark("source retirement");
        self.static_residency.remap_sources(&remap);
        profile.mark("static source remap");
        for placement in &mut self.placements {
            // Every surviving placement marked its source above.
            placement.source_index = remap[placement.source_index];
        }
        profile.mark("placement source remap");
    }
}

/// Creates independent animation/effect state only for a newly admitted owner.
pub(super) fn static_gpu_placement(
    placement: &ResidentM2Placement,
    source_index: usize,
    source: Option<&M2GpuSource>,
    animations: &AnimationDataCatalog,
    scene_time_ms: f32,
    random: &mut CrtRand,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let (playback, particles, ribbons) = match source {
        Some(source) => (
            Some(M2Playback::default_sequence(
                &source.model,
                animations,
                scene_time_ms as u32,
                random,
            )?),
            stock_particle_simulations(&source.model),
            source
                .model
                .animations()
                .ribbons()
                .iter()
                .map(M2RibbonTrail::new)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => (None, Vec::new(), Vec::new()),
    };
    Ok(M2GpuPlacement {
        ground_placement: None,
        scene_registration: None,
        rider_scale: 1.0,
        sound_lifetime: Default::default(),
        light_lifetime: Default::default(),
        entity_lighting: Default::default(),
        placement_valid: true,
        scene_indoor_fog: false,
        world_model_state: None,
        source_index,
        local_transform: placement.transform(),
        transform: placement.transform(),
        orientation: M2ModelOrientation::Authored,
        glue_parent_attachment: None,
        owner: M2GpuPlacementOwner::Static(placement.owner()),
        flags: placement.flags(),
        color: placement.color(),
        opacity: 1.0,
        entity_opacity: None,
        particle_colors: None,
        playback: playback.map(super::M2PlaybackStorage::Local),
        unit_animation: None,
        unit_presentation: None,
        mount_key: None,
        item_identity: None,
        particles,
        ribbons,
        last_effect_time_ms: scene_time_ms as u32,
        unit_effect: None,
    })
}
