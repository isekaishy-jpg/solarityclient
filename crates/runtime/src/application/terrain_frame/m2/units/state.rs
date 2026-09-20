//! Ordered authoritative unit updates use current simulation ownership.

use super::super::playback::M2PlaybackStorage;
use super::super::unit_registration::UnitSceneRegistration;
use super::super::{M2Frame, M2GpuPlacementOwner, RuntimeTerrainFrameError};
use super::super::{UnitGroundPlacement, unit_placement_transform};
use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentPlayerFrameInput,
};
use crate::random::CrtRand;
use std::rc::Rc;

impl M2Frame {
    /// Simulation storage tracks the first dynamic owner through every mutation,
    /// including the interval before rendering publishes new topology metadata.
    fn dynamic_placement_index(&self, owner: M2GpuPlacementOwner) -> Option<usize> {
        self.placements.owner_indices(owner).next()
    }

    /// Updates authoritative player movement and base animation in place.
    pub(in super::super::super) fn update_player_state(
        &mut self,
        input: ResidentPlayerFrameInput<'_>,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let transform = if let Some(mount) = input.mount() {
            let transform =
                unit_placement_transform(input.world_transform(), mount.object_scale())?;
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::PlayerMount { guid: input.guid() })
                .ok_or(RuntimeTerrainFrameError::MissingPlayerMountM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.ground_placement =
                input.unit_animation().map(|animation| UnitGroundPlacement {
                    position: input.world_transform().position(),
                    scale: mount.object_scale(),
                    owner: Rc::clone(animation),
                });
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                return Ok(());
            };
            if let Some(animation) = input.unit_animation() {
                animation.synchronize(animation_time_ms as u32, random)?;
            } else if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
                playback.select_mount_animation(
                    &source.model,
                    mount.animation().animation_id(),
                    None,
                    (1., 0),
                    animation_time_ms,
                    solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                    random,
                )?;
            }
            transform
        } else {
            unit_placement_transform(input.world_transform(), input.object_scale())?
        };
        let placement_index = self
            .dynamic_placement_index(M2GpuPlacementOwner::PlayerBody { guid: input.guid() })
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2Placement { guid: input.guid() })?;
        let placement = &mut self.placements[placement_index];
        placement.transform = transform;
        placement.local_transform = transform;
        placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
        placement.scene_registration = Some(UnitSceneRegistration::new(
            input
                .mount()
                .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
            transform,
        )?);
        let Some(source) = self.sources[placement.source_index].as_ref() else {
            return Ok(());
        };
        if let Some(animation) = input.unit_animation() {
            placement.ground_placement = input.mount().is_none().then_some(UnitGroundPlacement {
                position: input.world_transform().position(),
                scale: input.object_scale(),
                owner: Rc::clone(animation),
            });
            animation.synchronize(animation_time_ms as u32, random)?;
            placement.unit_animation = Some(Rc::clone(animation));
            placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
            return Ok(());
        }
        if let Some(mut playback) = placement
            .playback
            .as_mut()
            .map(M2PlaybackStorage::borrow_mut)
        {
            playback.select_animation(
                &source.model,
                input.animation().animation_id(),
                animation_time_ms,
                random,
            )?;
        }
        Ok(())
    }

    /// Updates every authoritative creature transform and selected animation.
    pub(in super::super::super) fn update_creature_states(
        &mut self,
        inputs: &[ResidentCreatureFrameInput<'_>],
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for input in inputs {
            let transform = if let Some(mount) = input.mount() {
                let transform =
                    unit_placement_transform(input.world_transform(), mount.object_scale())?;
                let placement_index = self
                    .dynamic_placement_index(M2GpuPlacementOwner::CreatureMount {
                        guid: input.guid(),
                    })
                    .ok_or(RuntimeTerrainFrameError::MissingCreatureMountM2Placement {
                        guid: input.guid(),
                    })?;
                let placement = &mut self.placements[placement_index];
                placement.transform = transform;
                placement.local_transform = transform;
                placement.ground_placement =
                    input.unit_animation().map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: mount.object_scale(),
                        owner: Rc::clone(animation),
                    });
                let Some(source) = self.sources[placement.source_index].as_ref() else {
                    continue;
                };
                if let Some(animation) = input.unit_animation() {
                    animation.synchronize(animation_time_ms as u32, random)?;
                } else if let Some(mut playback) = placement
                    .playback
                    .as_mut()
                    .map(M2PlaybackStorage::borrow_mut)
                {
                    playback.select_mount_animation(
                        &source.model,
                        mount.animation().animation_id(),
                        None,
                        (1., 0),
                        animation_time_ms,
                        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                        random,
                    )?;
                }
                transform
            } else {
                unit_placement_transform(input.world_transform(), input.object_scale())?
            };
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::CreatureBody { guid: input.guid() })
                .ok_or(RuntimeTerrainFrameError::MissingCreatureM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
            placement.scene_registration = Some(UnitSceneRegistration::new(
                input
                    .mount()
                    .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(animation) = input.unit_animation() {
                placement.ground_placement =
                    input.mount().is_none().then_some(UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
                animation.synchronize(animation_time_ms as u32, random)?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
                continue;
            }
            if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
                playback.select_animation(
                    &source.model,
                    input.animation().animation_id(),
                    animation_time_ms,
                    random,
                )?;
            }
        }
        Ok(())
    }

    /// Updates every authoritative remote-player transform and animation.
    pub(in super::super::super) fn update_remote_player_states(
        &mut self,
        inputs: &[ResidentPlayerFrameInput<'_>],
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for input in inputs {
            let transform = if let Some(mount) = input.mount() {
                let transform =
                    unit_placement_transform(input.world_transform(), mount.object_scale())?;
                let placement_index = self
                    .dynamic_placement_index(M2GpuPlacementOwner::RemotePlayerMount {
                        guid: input.guid(),
                    })
                    .ok_or(
                        RuntimeTerrainFrameError::MissingRemotePlayerMountM2Placement {
                            guid: input.guid(),
                        },
                    )?;
                let placement = &mut self.placements[placement_index];
                placement.transform = transform;
                placement.local_transform = transform;
                placement.ground_placement =
                    input.unit_animation().map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: mount.object_scale(),
                        owner: Rc::clone(animation),
                    });
                let Some(source) = self.sources[placement.source_index].as_ref() else {
                    continue;
                };
                if let Some(animation) = input.unit_animation() {
                    animation.synchronize(animation_time_ms as u32, random)?;
                } else if let Some(mut playback) = placement
                    .playback
                    .as_mut()
                    .map(M2PlaybackStorage::borrow_mut)
                {
                    playback.select_mount_animation(
                        &source.model,
                        mount.animation().animation_id(),
                        None,
                        (1., 0),
                        animation_time_ms,
                        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                        random,
                    )?;
                }
                transform
            } else {
                unit_placement_transform(input.world_transform(), input.object_scale())?
            };
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::RemotePlayerBody {
                    guid: input.guid(),
                })
                .ok_or(RuntimeTerrainFrameError::MissingRemotePlayerM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
            placement.scene_registration = Some(UnitSceneRegistration::new(
                input
                    .mount()
                    .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(animation) = input.unit_animation() {
                placement.ground_placement =
                    input.mount().is_none().then_some(UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
                animation.synchronize(animation_time_ms as u32, random)?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
                continue;
            }
            if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
                playback.select_animation(
                    &source.model,
                    input.animation().animation_id(),
                    animation_time_ms,
                    random,
                )?;
            }
        }
        Ok(())
    }
}
