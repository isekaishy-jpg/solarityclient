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
        }
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
        scenes: impl Iterator<Item = &'a ResidentM2Scene> + Clone,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut profile = RuntimeFrameProfile::new("M2 residency publication");
        let requested = scenes
            .clone()
            .flat_map(ResidentM2Scene::placements)
            .map(|placement| placement.owner())
            .collect::<HashSet<_>>();
        profile.mark("requested owners");
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
        for scene in scenes {
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
        // Retirement already visits every live owner. Gather its source here
        // instead of scanning the large animation/effect records a second time.
        let mut remap = vec![usize::MAX; self.sources.len()];
        for placement in &added {
            remap[placement.source_index] = 0;
        }
        self.placements.retain(|placement| {
            let keep = match placement.owner {
                M2GpuPlacementOwner::Static(owner) => requested.contains(&owner),
                _ => true,
            };
            if keep {
                remap[placement.source_index] = 0;
            }
            keep
        });
        profile.mark("placement retirement");
        self.placements.extend(added);
        // Publish identities only after every new owner is ready, so a failed
        // resource preparation cannot make the next attempt skip that owner.
        self.static_residency.owners = requested;
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
