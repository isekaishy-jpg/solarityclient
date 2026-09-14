//! Authored unit callbacks run during the scene tick, before draw admission.

use super::super::{
    CrtRand, M2BonePoseOverrides, M2BoneTransforms, M2Frame, M2GpuPlacementOwner, M2Playback, Mat4,
    RuntimeTerrainFrameError, WorldCameraFrame, append_triggered_events, unit_effects,
};
use crate::application::model_playback::M2ExpiredVariation;

impl M2Frame {
    pub(in crate::application::terrain_frame::m2) fn advance_unit_callbacks(
        &mut self,
        camera: WorldCameraFrame,
        now: f32,
        random: &mut CrtRand,
        mut effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile_scope = solarity_profiling::profile!(
            "runtime.application.terrain_frame.m2.unit_scene.frame.advance_unit_callbacks"
        );
        for ordinal in 0..self.placement_visibility.dynamic_scene_indices().len() {
            let index = self.placement_visibility.dynamic_scene_indices()[ordinal];
            let placement = &self.placements[index];
            let mount = matches!(
                placement.owner,
                M2GpuPlacementOwner::PlayerMount { .. }
                    | M2GpuPlacementOwner::RemotePlayerMount { .. }
                    | M2GpuPlacementOwner::CreatureMount { .. }
            );
            if placement.unit_animation.is_none() && !mount {
                continue;
            }
            self.vehicle_passengers.refresh_callback_pose(
                index,
                self.placements.as_mut_slice(),
                &self.sources,
                &self.placement_visibility,
                &self.requested_items,
                camera.view(),
                now,
                random,
            )?;
            if self.vehicle_passengers.callbacks_disabled(index) {
                if let Some(playback) = &mut self.placements[index].playback {
                    playback.borrow_mut().previous_event_scene_time_ms = now as u32;
                }
                continue;
            }
            if mount {
                self.advance_mount_callbacks(
                    index,
                    camera,
                    now,
                    random,
                    effect_callback.as_deref_mut(),
                )?;
                continue;
            }
            if !self.prepare_rider_callback_transform(index, camera, now)? {
                // 832450 skips disabled attachment children. Their timers still
                // age; the next admitted scan sees only that scene's interval.
                if let Some(playback) = &mut self.placements[index].playback {
                    playback.borrow_mut().previous_event_scene_time_ms = now as u32;
                }
                continue;
            }
            let placement = &self.placements[index];
            let Some(animation) = &placement.unit_animation else {
                continue;
            };
            let Some(source) = &self.sources[placement.source_index] else {
                continue;
            };
            let body_pose = animation.body_pose();
            let transform = placement.transform;
            let mut event = |_: &mut M2Playback,
                             _: usize,
                             _: u32,
                             event: &M2ExpiredVariation,
                             random: &mut CrtRand| {
                self.bone_demand.clear();
                self.bone_demand
                    .events(&source.model, placement.owner, event.event_window);
                self.bone_samples_scratch.recompose(
                    source.model.animations(),
                    event.clock,
                    camera.view() * transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_transforms: body_pose.bone_transforms(),
                        bone_sequences: &event.bone_sequences,
                        ..Default::default()
                    },
                    self.bone_demand.bones(),
                )?;
                let first = self.triggered_events.len();
                append_triggered_events(
                    &mut self.triggered_events,
                    &source.model,
                    placement.owner,
                    transform,
                    &placement.sound_lifetime,
                    &self.bone_samples_scratch,
                    event.event_window,
                )?;
                if let Some(callback) = effect_callback.as_mut() {
                    for event in &self.triggered_events[first..] {
                        if let Some(request) = callback(event, animation, &source.model, transform)
                        {
                            self.unit_effects
                                .emit(request, &self.animations, now, random)?;
                        }
                    }
                }
                Ok(())
            };
            animation.advance_prepared_scene(now, random, Some(&mut event))?;
            if self.unit_effects.has_anchors(animation)
                && let Some(playback) = &placement.playback
            {
                let playback = playback.borrow();
                let clock = playback.sample_clock(now as u32);
                let sequences = playback.bone_sequence_clocks(&source.model, clock, now as u32);
                self.bone_demand.clear();
                self.unit_effects.request_anchor_bones(
                    animation,
                    &source.model,
                    &mut self.bone_demand,
                );
                self.bone_samples_scratch.recompose(
                    source.model.animations(),
                    clock,
                    camera.view() * transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_transforms: body_pose.bone_transforms(),
                        bone_sequences: &sequences,
                        ..Default::default()
                    },
                    self.bone_demand.bones(),
                )?;
                self.unit_effects.update_anchor(
                    animation,
                    &source.model,
                    &self.bone_samples_scratch,
                    transform,
                )?;
            }
        }
        Ok(())
    }

    fn prepare_rider_callback_transform(
        &mut self,
        index: usize,
        camera: WorldCameraFrame,
        now: f32,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let owner = self.placements[index].owner;
        let guid = match owner {
            M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } => guid,
            _ => return Ok(true),
        };
        if !self.mounted_guids.contains(&guid) {
            return Ok(true);
        }
        let parent = [
            M2GpuPlacementOwner::PlayerMount { guid },
            M2GpuPlacementOwner::RemotePlayerMount { guid },
            M2GpuPlacementOwner::CreatureMount { guid },
        ]
        .into_iter()
        .find_map(|owner| self.placement_visibility.dynamic_owner_index(owner))
        .ok_or(RuntimeTerrainFrameError::MissingMountM2AttachmentPose {
            guid,
            attachment_id: 0,
        })?;
        let placement = &self.placements[parent];
        if self.vehicle_passengers.callbacks_disabled(parent) {
            return Ok(false);
        }
        let Some(source) = &self.sources[placement.source_index] else {
            return Ok(false);
        };
        let Some(playback) = &placement.playback else {
            return Ok(false);
        };
        let playback = playback.borrow();
        let clock = playback.sample_clock(now as u32);
        let sequences = playback.bone_sequence_clocks(&source.model, clock, now as u32);
        self.bone_demand.clear();
        self.bone_demand.attachment(&source.model, 0);
        self.bone_samples_scratch.recompose(
            source.model.animations(),
            clock,
            camera.view() * placement.transform,
            M2BonePoseOverrides {
                model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                bone_sequences: &sequences,
                ..Default::default()
            },
            self.bone_demand.bones(),
        )?;
        let attachment = source.model.attachment(0).ok_or_else(|| {
            RuntimeTerrainFrameError::MissingMountM2Attachment {
                model: source.model.path().clone(),
                attachment_id: 0,
            }
        })?;
        let transform = self.bone_samples_scratch.attachment_transform(
            source.model.animations(),
            attachment,
            clock,
            placement.transform,
        )?;
        drop(playback);
        if let Some(transform) = transform {
            self.placements[index].transform =
                transform * Mat4::from_scale(glam::Vec3::splat(self.placements[index].rider_scale));
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
