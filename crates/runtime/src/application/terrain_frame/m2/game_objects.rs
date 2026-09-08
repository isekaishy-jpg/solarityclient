//! GPU instances borrow CPU-admitted object and attached-doodad animation owners.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::application::game_object_coordinator::{GameObjectFrameInput, GameObjectResource};
use crate::random::CrtRand;
use solarity_rendering::VulkanRenderer;

use super::{
    M2Frame, M2GpuPlacementOwner, M2Playback, M2PlaybackStorage, RuntimeTerrainFrameError,
    m2_gpu_placement, prepare_source,
};

impl M2Frame {
    pub(in crate::application::terrain_frame) fn synchronize_game_objects(
        &mut self,
        renderer: &mut VulkanRenderer,
        game_objects: GameObjectFrameInput<'_>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let sources = &self.sources;
        self.placements.retain(|placement| {
            let (identity, display_id, doodad_index) = match placement.owner {
                M2GpuPlacementOwner::GameObject {
                    identity,
                    display_id,
                    ..
                } => (identity, display_id, None),
                M2GpuPlacementOwner::GameObjectWorldModelDoodad {
                    identity,
                    display_id,
                    doodad_index,
                } => (identity, display_id, Some(doodad_index)),
                _ => return true,
            };
            let Some(instance) = game_objects
                .get(identity)
                .filter(|instance| instance.display_id() == display_id)
            else {
                return false;
            };
            let Some(gpu) = sources.get(placement.source_index).and_then(Option::as_ref) else {
                return false;
            };
            match (instance.resource(), doodad_index) {
                (Some(GameObjectResource::M2(cpu)), None) => Arc::ptr_eq(cpu.model(), &gpu.model),
                (Some(GameObjectResource::WorldModel(cpu)), Some(index)) => {
                    placement
                        .world_model_state
                        .as_ref()
                        .is_some_and(|previous| {
                            instance
                                .world_model_state()
                                .is_some_and(|current| Rc::ptr_eq(previous, &current))
                        })
                        && cpu.doodads().iter().any(|doodad| {
                            doodad.index == index && Arc::ptr_eq(doodad.source.model(), &gpu.model)
                        })
                }
                _ => false,
            }
        });
        let retained = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::GameObject {
                    identity,
                    display_id,
                    ..
                } => Some((identity, display_id, None)),
                M2GpuPlacementOwner::GameObjectWorldModelDoodad {
                    identity,
                    display_id,
                    doodad_index,
                } => Some((identity, display_id, Some(doodad_index))),
                _ => None,
            })
            .collect::<HashSet<_>>();
        // Authored texture sources may be shared by static and replicated
        // props. Character replacements use a different material domain even
        // when they happen to reference the same decoded M2 generation.
        let mut sources = HashMap::new();
        for placement in &self.placements {
            if matches!(
                placement.owner,
                M2GpuPlacementOwner::Static(_)
                    | M2GpuPlacementOwner::GameObject { .. }
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            ) && let Some(source) = self.sources[placement.source_index].as_ref()
            {
                sources.insert(Arc::as_ptr(&source.model), placement.source_index);
            }
        }
        let scene_time_ms = self.animation_time_ms();
        for instance in game_objects.instances() {
            let (Some(resource), Some(resolved)) = (instance.resource(), instance.placement())
            else {
                continue;
            };
            match resource {
                GameObjectResource::M2(cpu) => {
                    if retained.contains(&(instance.identity(), instance.display_id(), None)) {
                        continue;
                    }
                    let source_index = if let Some(index) = sources.get(&Arc::as_ptr(cpu.model())) {
                        *index
                    } else {
                        let Some(gpu) = prepare_source(renderer, cpu)? else {
                            continue;
                        };
                        let index = self.sources.len();
                        sources.insert(Arc::as_ptr(&gpu.model), index);
                        self.sources.push(Some(gpu));
                        index
                    };
                    let playback = if let Some(behavior) = instance.behavior() {
                        let Some(playback) = behavior.playback() else {
                            continue;
                        };
                        M2PlaybackStorage::Shared(playback)
                    } else if let Some(model) = instance.transport_model() {
                        let Some(playback) = model.playback() else {
                            continue;
                        };
                        M2PlaybackStorage::Shared(playback)
                    } else {
                        let mut playback = M2Playback::unstarted(0, scene_time_ms as u32);
                        playback.select_game_object_state(
                            cpu.model(),
                            game_objects.animations(),
                            instance.state(),
                            scene_time_ms as u32,
                            random,
                        )?;
                        M2PlaybackStorage::Local(playback)
                    };
                    self.placements.push(m2_gpu_placement(
                        source_index,
                        resolved.matrix(),
                        M2GpuPlacementOwner::GameObject {
                            guid: instance.guid(),
                            identity: instance.identity(),
                            display_id: instance.display_id(),
                        },
                        cpu.model(),
                        Some(playback),
                        None,
                        scene_time_ms as u32,
                    )?);
                }
                GameObjectResource::WorldModel(cpu) => {
                    let Some(state) = instance.world_model_state() else {
                        continue;
                    };
                    for doodad in cpu.doodads() {
                        if retained.contains(&(
                            instance.identity(),
                            instance.display_id(),
                            Some(doodad.index),
                        )) {
                            continue;
                        }
                        let model = doodad.source.model();
                        let source_index = if let Some(index) = sources.get(&Arc::as_ptr(model)) {
                            *index
                        } else {
                            let Some(gpu) = prepare_source(renderer, &doodad.source)? else {
                                continue;
                            };
                            let index = self.sources.len();
                            sources.insert(Arc::as_ptr(&gpu.model), index);
                            self.sources.push(Some(gpu));
                            index
                        };
                        let mut placement = m2_gpu_placement(
                            source_index,
                            resolved.matrix() * doodad.local_transform,
                            M2GpuPlacementOwner::GameObjectWorldModelDoodad {
                                identity: instance.identity(),
                                display_id: instance.display_id(),
                                doodad_index: doodad.index,
                            },
                            model,
                            state.playback(doodad.index).map(M2PlaybackStorage::Shared),
                            None,
                            scene_time_ms as u32,
                        )?;
                        let authored = &cpu.model().doodads()[doodad.index];
                        placement.flags = u16::from(authored.flags());
                        placement.color = authored.color();
                        placement.local_transform = doodad.local_transform;
                        placement.world_model_state = Some(Rc::clone(&state));
                        self.placements.push(placement);
                    }
                }
            }
        }
        self.placement_topology_dirty = true;
        self.compact_sources();
        Ok(())
    }

    pub(in crate::application::terrain_frame) fn update_game_object_states(
        &mut self,
        game_objects: GameObjectFrameInput<'_>,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for placement in &mut self.placements {
            let (identity, doodad) = match placement.owner {
                M2GpuPlacementOwner::GameObject { identity, .. } => (identity, false),
                M2GpuPlacementOwner::GameObjectWorldModelDoodad { identity, .. } => {
                    (identity, true)
                }
                _ => continue,
            };
            let Some(instance) = game_objects.get(identity) else {
                placement.placement_valid = false;
                continue;
            };
            let resolved = instance.placement();
            placement.placement_valid = resolved.is_some();
            if let Some(resolved) = resolved {
                if doodad {
                    placement.transform = resolved.matrix() * placement.local_transform;
                } else {
                    placement.local_transform = resolved.matrix();
                    placement.transform = resolved.matrix();
                }
            }
            if !doodad
                && instance.behavior().is_none()
                && instance.transport_model().is_none()
                && let Some(source) = self.sources[placement.source_index].as_ref()
                && let Some(mut playback) = placement
                    .playback
                    .as_mut()
                    .map(M2PlaybackStorage::borrow_mut)
            {
                playback.select_game_object_state(
                    &source.model,
                    game_objects.animations(),
                    instance.state(),
                    animation_time_ms as u32,
                    random,
                )?;
            }
        }
        Ok(())
    }
}
