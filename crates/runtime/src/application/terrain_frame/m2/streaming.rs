//! Static M2 placement ownership across overlapping resident ADTs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use solarity_rendering::{M2ModelOrientation, M2RibbonTrail, VulkanRenderer};

use crate::application::terrain_coordinator::m2_residency::{ResidentM2Placement, ResidentM2Scene};
use crate::random::CrtRand;

use super::{
    M2AnimationBinding, M2Frame, M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, M2Playback,
    RuntimeTerrainFrameError, prepare_source, stock_particle_simulations,
};

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
        let requested = scenes
            .clone()
            .flat_map(ResidentM2Scene::placements)
            .map(|placement| placement.owner())
            .collect::<HashSet<_>>();
        let mut retained = HashSet::with_capacity(requested.len());
        // Source reuse is confined to static owners: character texture
        // replacements can share an M2 model but represent different materials.
        let mut sources = HashMap::new();
        for placement in &self.placements {
            if let M2GpuPlacementOwner::Static(owner) = placement.owner {
                retained.insert(owner);
                if let Some(source) = self.sources[placement.source_index].as_ref() {
                    sources.insert(Arc::as_ptr(&source.model), placement.source_index);
                }
            }
        }
        let mut added = Vec::new();
        for scene in scenes {
            for placement in scene.placements() {
                if !retained.insert(placement.owner()) {
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
                    sources.insert(identity, index);
                    index
                };
                added.push(static_gpu_placement(
                    placement,
                    source_index,
                    self.sources[source_index].as_ref(),
                    self.animation_time_ms(),
                    random,
                )?);
            }
        }
        self.placements.retain(|placement| match placement.owner {
            M2GpuPlacementOwner::Static(owner) => requested.contains(&owner),
            _ => true,
        });
        self.placements.extend(added);
        self.placement_topology_dirty = true;
        self.compact_sources();
        Ok(())
    }

    /// Drops unreferenced sources and remaps surviving static and dynamic slots.
    /// Empty geometry is still an occupied source when a placement references it.
    pub(super) fn compact_sources(&mut self) {
        let mut remap = vec![usize::MAX; self.sources.len()];
        for placement in &self.placements {
            remap[placement.source_index] = 0;
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
        for placement in &mut self.placements {
            // Every surviving placement marked its source above.
            placement.source_index = remap[placement.source_index];
        }
    }
}

/// Creates independent animation/effect state only for a newly admitted owner.
pub(super) fn static_gpu_placement(
    placement: &ResidentM2Placement,
    source_index: usize,
    source: Option<&M2GpuSource>,
    scene_time_ms: f32,
    random: &mut CrtRand,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let (playback, particles, ribbons) = match source {
        Some(source) => (
            M2Playback::new_at(&source.model, 0, scene_time_ms, random)?,
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
        placement_valid: true,
        source_index,
        local_transform: placement.transform(),
        transform: placement.transform(),
        orientation: M2ModelOrientation::Authored,
        animation_binding: M2AnimationBinding::Independent,
        glue_parent_attachment: None,
        owner: M2GpuPlacementOwner::Static(placement.owner()),
        flags: placement.flags(),
        color: placement.color(),
        opacity: 1.0,
        particle_colors: None,
        playback: playback.map(super::M2PlaybackStorage::Local),
        particles,
        ribbons,
    })
}
