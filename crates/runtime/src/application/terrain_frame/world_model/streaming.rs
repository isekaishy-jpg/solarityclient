//! Shared MODF ownership as neighboring ADTs enter and leave the world.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use solarity_rendering::{PlacedWorldModelDrawPlan, VulkanRenderer};

use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;

use super::{
    RuntimeTerrainFrameError, WorldModelFrame, WorldModelGpuPlacement, WorldModelGpuPlacementOwner,
    prepare_gpu_source,
};

impl WorldModelFrame {
    /// Reuses shared root/group resources and keeps one transform per MODF ID.
    pub(in crate::application::terrain_frame) fn synchronize_static_scenes<'a>(
        &mut self,
        renderer: &mut VulkanRenderer,
        scenes: impl Iterator<Item = &'a ResidentWorldModelScene> + Clone,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let requested = scenes
            .clone()
            .flat_map(ResidentWorldModelScene::placements)
            .map(|placement| placement.unique_id())
            .collect::<HashSet<_>>();
        let mut retained = HashSet::with_capacity(requested.len());
        let mut sources = HashMap::new();
        for placement in &self.placements {
            if let WorldModelGpuPlacementOwner::Static { unique_id } = placement.owner {
                retained.insert(unique_id);
                if let Some(source) = self.sources[placement.source_index].as_ref() {
                    sources.insert(Arc::as_ptr(&source.model), placement.source_index);
                }
            }
        }
        let mut added = Vec::new();
        for scene in scenes {
            for placement in scene.placements() {
                if !retained.insert(placement.unique_id()) {
                    continue;
                }
                let source = scene.sources().get(placement.source_index()).ok_or(
                    RuntimeTerrainFrameError::WorldModelSourceIndex {
                        source_index: placement.source_index(),
                        source_count: scene.sources().len(),
                    },
                )?;
                let identity = Arc::as_ptr(source.model());
                let source_index = if let Some(&index) = sources.get(&identity) {
                    index
                } else {
                    let gpu = prepare_gpu_source(renderer, source, self.filtering, self.base_mip)?;
                    let index = self.sources.len();
                    self.sources.push(Some(gpu));
                    sources.insert(identity, index);
                    index
                };
                let gpu = self.sources[source_index].as_ref().ok_or(
                    RuntimeTerrainFrameError::WorldModelSourceIndex {
                        source_index,
                        source_count: self.sources.len(),
                    },
                )?;
                let plan = PlacedWorldModelDrawPlan::prepare(
                    Arc::clone(&gpu.plan),
                    placement.position(),
                    placement.rotation_degrees(),
                    1.0,
                )?;
                added.push(WorldModelGpuPlacement {
                    source_index,
                    owner: WorldModelGpuPlacementOwner::Static {
                        unique_id: placement.unique_id(),
                    },
                    plan,
                    visible_draw_indices: Vec::with_capacity(gpu.plan.draws().len()),
                });
            }
        }
        self.placements.retain(|placement| match placement.owner {
            WorldModelGpuPlacementOwner::Static { unique_id } => requested.contains(&unique_id),
            WorldModelGpuPlacementOwner::Transport { .. } => true,
        });
        self.placements.extend(added);
        self.compact_sources();
        Ok(())
    }

    /// Removes unused sources without accumulating holes during world traversal.
    fn compact_sources(&mut self) {
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
            placement.source_index = remap[placement.source_index];
        }
    }
}
