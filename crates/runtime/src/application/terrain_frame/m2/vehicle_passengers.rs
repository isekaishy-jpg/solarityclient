//! Resolve passenger model ancestry before culling, lighting and effect anchors.

use super::{
    M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, RuntimeTerrainFrameError,
    UnitAnimationBehavior, held_item_finger_pose, visibility::M2PlacementVisibility,
};
use crate::application::unit_animation::passenger::UnitPassengerModelInput;
use crate::random::CrtRand;
use glam::{Mat4, Vec3};
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::{
    CharacterAttachmentPoint, M2AnimationClock, M2BonePose, M2BonePoseOverrides,
};
use solarity_systems::{
    VehiclePassengerPhase as Phase, VehicleSeatPose, vehicle_entry_target, vehicle_seat_attachment,
    vehicle_seat_transform,
};
use std::{collections::HashMap, rc::Rc};

#[derive(Default)]
pub(super) struct M2VehiclePassengers {
    state: Vec<u8>,
    hidden: Vec<bool>,
    parents: HashMap<usize, usize>,
    chain: Vec<(usize, bool)>,
    palettes: HashMap<usize, ParentPalette>,
}

#[derive(Default)]
struct ParentPalette {
    bones: M2BonePose,
    clock: Option<M2AnimationClock>,
}

#[derive(Clone, Copy, PartialEq)]
enum SeatPosePurpose {
    InitialTarget,
    CurrentTarget,
    Attached,
}

impl M2VehiclePassengers {
    pub fn invalidate(&mut self) {
        self.palettes.clear();
    }

    pub fn hidden(&self, index: usize) -> bool {
        self.hidden.get(index).copied().unwrap_or(false)
    }

    pub fn parents(&self) -> &HashMap<usize, usize> {
        &self.parents
    }

    /// Native F60 advances before the unit animation callback. Existing model
    /// clocks supply its initial seat target; the later pose pass uses new bones.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_timing(
        &mut self,
        placements: &mut [M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        visibility: &M2PlacementVisibility,
        requested_items: &[(u64, CharacterAttachmentPoint)],
        view: Mat4,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.hidden.resize(placements.len(), false);
        for &index in visibility.dynamic_indices() {
            placements[index].passenger_playback_advance = None;
        }
        for palette in self.palettes.values_mut() {
            palette.clock = None;
        }
        for &index in visibility.dynamic_indices() {
            let Some(owner) = unit_owner(&placements[index]).cloned() else {
                continue;
            };
            if !owner.passenger_needs_advance(now as u32)
                || model_index(visibility, placements, owner.identity()) != Some(index)
            {
                continue;
            }
            let Some(input) = owner.passenger_input() else {
                continue;
            };
            let fallback = placements[index].local_transform.w_axis.truncate();
            let target = if matches!(owner.passenger_phase(), Phase::Entering | Phase::EnterDelay) {
                self.seat_pose(
                    index,
                    input,
                    SeatPosePurpose::InitialTarget,
                    placements,
                    sources,
                    visibility,
                    requested_items,
                    view,
                    now,
                    random,
                )?
                .map_or(fallback, |matrix| matrix.w_axis.truncate())
            } else {
                fallback
            };
            owner.advance_passenger(now as u32, target, input.parent_velocity);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        placements: &mut [M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        visibility: &M2PlacementVisibility,
        requested_items: &[(u64, CharacterAttachmentPoint)],
        view: Mat4,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.state.resize(placements.len(), 0);
        self.state.fill(0);
        self.hidden.resize(placements.len(), false);
        self.hidden.fill(false);
        self.parents.clear();
        for palette in self.palettes.values_mut() {
            palette.clock = None;
        }
        for &index in visibility.dynamic_indices() {
            let Some(owner) = unit_owner(&placements[index]) else {
                continue;
            };
            if model_index(visibility, placements, owner.identity()) != Some(index)
                || self.state[index] != 0
            {
                continue;
            }
            if owner.passenger_input().is_none() {
                owner.publish_passenger_transform(placements[index].transform);
                continue;
            }
            self.chain.clear();
            self.chain.push((index, false));
            while let Some((next, expanded)) = self.chain.pop() {
                if self.state[next] == 2 {
                    continue;
                }
                if expanded {
                    self.resolve(
                        next,
                        placements,
                        sources,
                        visibility,
                        requested_items,
                        view,
                        now,
                        random,
                    )?;
                    self.state[next] = 2;
                    continue;
                }
                if self.state[next] == 1 {
                    // Corrupt ancestry cannot recurse or propagate stale matrices.
                    for &(index, _) in &self.chain {
                        self.state[index] = 2;
                    }
                    self.state[next] = 2;
                    self.chain.clear();
                    break;
                }
                self.state[next] = 1;
                self.chain.push((next, true));
                let Some(owner) = unit_owner(&placements[next]) else {
                    continue;
                };
                for input in [owner.passenger_input(), owner.passenger_pose_input()]
                    .into_iter()
                    .flatten()
                {
                    if input.parent_live
                        && input.seat.is_some()
                        && let Some(parent) = model_index(visibility, placements, input.parent)
                        && unit_owner(&placements[parent])
                            .is_some_and(|owner| owner.passenger_input().is_some())
                    {
                        self.chain.push((parent, false));
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve(
        &mut self,
        index: usize,
        placements: &mut [M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        visibility: &M2PlacementVisibility,
        requested_items: &[(u64, CharacterAttachmentPoint)],
        view: Mat4,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(owner) = unit_owner(&placements[index]).cloned() else {
            return Ok(());
        };
        let Some(input) = owner.passenger_input() else {
            return Ok(());
        };
        let scale = placements[index].ground_placement.as_ref().map_or_else(
            || placements[index].local_transform.x_axis.truncate().length(),
            |ground| ground.scale,
        );
        let fallback = placements[index].local_transform.w_axis.truncate();
        let target = if matches!(owner.passenger_phase(), Phase::EnterDelay | Phase::Entering) {
            self.seat_pose(
                index,
                input,
                SeatPosePurpose::CurrentTarget,
                placements,
                sources,
                visibility,
                requested_items,
                view,
                now,
                random,
            )?
            .map_or(fallback, |matrix| matrix.w_axis.truncate())
        } else {
            fallback
        };
        let pose = if let Some(input) = owner.passenger_pose_input() {
            self.seat_pose(
                index,
                input,
                SeatPosePurpose::Attached,
                placements,
                sources,
                visibility,
                requested_items,
                view,
                now,
                random,
            )?
        } else if owner.passenger_phase() == Phase::Detached {
            Some(placements[index].local_transform)
        } else {
            owner.passenger_transition_transform(now as u32, target, scale)
        };
        if let Some(transform) = pose {
            if !transform.is_finite() {
                return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
            }
            placements[index].transform = transform;
            owner.publish_passenger_transform(transform);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn seat_pose(
        &mut self,
        index: usize,
        input: UnitPassengerModelInput,
        purpose: SeatPosePurpose,
        placements: &mut [M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        visibility: &M2PlacementVisibility,
        requested_items: &[(u64, CharacterAttachmentPoint)],
        view: Mat4,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<Option<Mat4>, RuntimeTerrainFrameError> {
        let Some(owner) = unit_owner(&placements[index]).cloned() else {
            return Ok(None);
        };
        let inherit = purpose == SeatPosePurpose::Attached;
        if !input.parent_live && input.seat.is_some() {
            return Ok(if inherit {
                owner.passenger_last_transform()
            } else {
                None
            });
        }
        // 74A7F0 uses ordinary world position and unit facing when the seat row
        // or parent model is absent. F60 still bypasses ordinary terrain tilt.
        let parent = model_index(visibility, placements, input.parent);
        let Some((seat, parent)) = input.seat.zip(parent) else {
            return Ok(Some(placements[index].local_transform));
        };
        let Some(source) = sources[placements[parent].source_index].as_ref() else {
            return Ok(None);
        };
        // The movement update has already replaced placement inputs. Native
        // duration initialization still reads the previous CM2Model placement.
        let parent_transform = if purpose == SeatPosePurpose::InitialTarget {
            unit_owner(&placements[parent])
                .and_then(|owner| owner.passenger_last_transform())
                .unwrap_or(placements[parent].transform)
        } else {
            placements[parent].transform
        };
        let attachment_id = vehicle_seat_attachment(seat.attachment_id());
        let attachment = attachment_id.and_then(|id| source.model.attachment(id));
        let attached = if let Some(attachment) = attachment {
            let palette = self.palettes.entry(parent).or_default();
            if palette.clock.is_none() {
                let placement = &mut placements[parent];
                let clock = if let Some(clock) = placement
                    .unit_animation
                    .as_ref()
                    .and_then(|owner| owner.scene_clock())
                {
                    clock
                } else if let Some(advance) = placement.passenger_playback_advance.as_ref() {
                    advance.clock
                } else if let Some(playback) = placement.playback.as_mut() {
                    let advance = playback.borrow_mut().clock(&source.model, now, random)?;
                    let clock = advance.clock;
                    placement.passenger_playback_advance = Some(advance);
                    clock
                } else {
                    return Ok(None);
                };
                let body = placement
                    .unit_animation
                    .as_ref()
                    .map(|owner| owner.body_pose());
                let sequences = placement
                    .unit_animation
                    .as_ref()
                    .and_then(|owner| owner.bone_sequences(clock, now as u32));
                let fingers =
                    held_item_finger_pose(requested_items, placement.owner).and_then(|hands| {
                        let sequence = source.model.animations().sequence_for_variation(15, 0)?;
                        let global_tick = placement
                            .playback
                            .as_ref()?
                            .borrow()
                            .global_tick(now as u32);
                        Some((
                            M2AnimationClock::new_with_global_tick(sequence, 0., global_tick),
                            hands,
                        ))
                    });
                palette.bones.recompose_with_overrides(
                    source.model.animations(),
                    clock,
                    view * parent_transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_transforms: body.as_ref().map_or(&[], |body| body.bone_transforms()),
                        bone_sequences: sequences
                            .as_ref()
                            .map_or(&[], |sequences| sequences.as_slice()),
                        finger_pose: fingers,
                    },
                )?;
                palette.clock = Some(clock);
            }
            let Some(clock) = palette.clock else {
                return Ok(None);
            };
            let enabled = palette.bones.attachment_transform(
                source.model.animations(),
                attachment,
                clock,
                parent_transform,
            )?;
            if inherit {
                self.hidden[index] = enabled.is_none()
                    || self.hidden[parent]
                    || placements[parent]
                        .entity_opacity
                        .as_ref()
                        .is_some_and(|owner| owner.hidden());
                self.parents.insert(index, parent);
            }
            // 831410 supplies the bone matrix independently of the attachment
            // enable channel; attached models inherit that channel's visibility.
            Some(
                parent_transform
                    * palette.bones.transforms()[usize::from(attachment.bone_index())]
                    * Mat4::from_translation(attachment.position()),
            )
        } else {
            None
        };
        let passenger_scale = placements[index].ground_placement.as_ref().map_or_else(
            || placements[index].local_transform.x_axis.truncate().length(),
            |ground| ground.scale,
        );
        let vehicle_scale = placements[parent].ground_placement.as_ref().map_or_else(
            || {
                placements[parent]
                    .local_transform
                    .x_axis
                    .truncate()
                    .length()
            },
            |ground| ground.scale,
        );
        let parent_transition = unit_owner(&placements[parent])
            .filter(|owner| !matches!(owner.passenger_phase(), Phase::Detached | Phase::Seated));
        let vehicle_position = if parent_transition.is_some() {
            parent_transform.w_axis.truncate()
        } else {
            input.parent_pose.position()
        };
        let vehicle_yaw = parent_transition
            .and_then(|owner| owner.passenger_last_yaw())
            .unwrap_or(input.parent_pose.orientation());
        let pose = VehicleSeatPose {
            passenger_yaw: owner.passenger_seat_yaw(
                owner.body_pose().placement_yaw - input.parent_pose.orientation(),
                inherit,
            ),
            rotation: Vec3::from_array(seat.passenger_rotation()),
            offset: Vec3::from_array(seat.attachment_offset()),
            passenger_anchor: sources[placements[index].source_index]
                .as_ref()
                .and_then(|source| owner.passenger_anchor(&source.model)),
            passenger_scale,
            vehicle_scale,
            attachment: attached,
            vehicle_position,
            vehicle_yaw,
        };
        let transform = if inherit {
            vehicle_seat_transform(pose)
        } else {
            Mat4::from_translation(vehicle_entry_target(
                pose,
                parent_transform,
                input.parent_frame,
                parent_transition.is_some(),
            ))
        };
        if !transform.is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
        }
        Ok(Some(transform))
    }
}

fn unit_owner(placement: &M2GpuPlacement) -> Option<&Rc<UnitAnimationBehavior>> {
    placement.unit_animation.as_ref().or_else(|| {
        placement
            .ground_placement
            .as_ref()
            .map(|ground| &ground.owner)
    })
}

/// 6E6F80 returns the mount when present, then the unit's body model.
fn model_index(
    visibility: &M2PlacementVisibility,
    placements: &[M2GpuPlacement],
    identity: WorldObjectIdentity,
) -> Option<usize> {
    let guid = identity.guid();
    [
        M2GpuPlacementOwner::PlayerMount { guid },
        M2GpuPlacementOwner::RemotePlayerMount { guid },
        M2GpuPlacementOwner::CreatureMount { guid },
        M2GpuPlacementOwner::PlayerBody { guid },
        M2GpuPlacementOwner::RemotePlayerBody { guid },
        M2GpuPlacementOwner::CreatureBody { guid },
    ]
    .into_iter()
    .filter_map(|owner| visibility.dynamic_owner_index(owner))
    .find(|index| unit_owner(&placements[*index]).is_some_and(|owner| owner.identity() == identity))
}
