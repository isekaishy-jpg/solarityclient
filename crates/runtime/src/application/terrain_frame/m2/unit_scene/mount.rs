//! 73D5D0 registers the unit's authored-event adapter on its mount too.

use super::*;
use glam::Vec3;

impl M2Frame {
    pub(super) fn advance_mount_callbacks(
        &mut self,
        index: usize,
        camera: WorldCameraFrame,
        now: f32,
        random: &mut CrtRand,
        mut effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let body_owner = match self.placements[index].owner {
            M2GpuPlacementOwner::PlayerMount { guid } => M2GpuPlacementOwner::PlayerBody { guid },
            M2GpuPlacementOwner::RemotePlayerMount { guid } => {
                M2GpuPlacementOwner::RemotePlayerBody { guid }
            }
            M2GpuPlacementOwner::CreatureMount { guid } => {
                M2GpuPlacementOwner::CreatureBody { guid }
            }
            _ => return Ok(()),
        };
        let body = self
            .placement_visibility
            .dynamic_owner_index(body_owner)
            .and_then(|index| {
                let body = &self.placements[index];
                Some((
                    Rc::clone(body.unit_animation.as_ref()?),
                    self.sources[body.source_index].as_ref()?,
                    body.rider_scale,
                ))
            });
        let placement = &mut self.placements[index];
        let Some(source) = &self.sources[placement.source_index] else {
            return Ok(());
        };
        let Some(playback) = &mut placement.playback else {
            return Ok(());
        };
        let transform = placement.transform;
        let mut event = |playback: &mut M2Playback,
                         _: usize,
                         _: u32,
                         event: &M2ExpiredVariation,
                         random: &mut CrtRand| {
            self.bone_pose_scratch.recompose_with_overrides(
                source.model.animations(),
                event.clock,
                camera.view() * transform,
                M2BonePoseOverrides {
                    model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                    bone_sequences: &event.bone_sequences,
                    ..Default::default()
                },
            )?;
            let first = self.triggered_events.len();
            append_triggered_events(
                &mut self.triggered_events,
                &source.model,
                placement.owner,
                transform,
                &placement.sound_lifetime,
                &self.bone_pose_scratch,
                event.event_window,
            )?;
            let Some((animation, body_source, rider_scale)) = &body else {
                return Ok(());
            };
            let Some(callback) = effect_callback.as_mut() else {
                return Ok(());
            };
            // 734A40 preserves the emitting model's captured event position.
            // 6F9260 separately queries attachment 17/19 on Unit_C's body+B4.
            // A tied completion may already have changed the mount timer, so
            // this body query samples its current pose without another scan.
            let clock = playback.sample_clock(now as u32);
            let sequences = playback.bone_sequence_clocks(&source.model, clock, now as u32);
            if clock != event.clock || sequences != event.bone_sequences {
                self.bone_pose_scratch.recompose_with_overrides(
                    source.model.animations(),
                    clock,
                    camera.view() * transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_sequences: &sequences,
                        ..Default::default()
                    },
                )?;
            }
            let attachment = source.model.attachment(0).ok_or_else(|| {
                RuntimeTerrainFrameError::MissingMountM2Attachment {
                    model: source.model.path().clone(),
                    attachment_id: 0,
                }
            })?;
            let bone = self
                .bone_pose_scratch
                .transforms()
                .get(usize::from(attachment.bone_index()))
                .ok_or(solarity_rendering::M2BonePoseError::AttachmentBoneIndex {
                    requested: attachment.bone_index(),
                    available: self.bone_pose_scratch.transforms().len(),
                })?;
            // Bone queries do not test an attachment's enable channel. That
            // channel governs the later rider callback traversal separately.
            let body_transform = transform
                * *bone
                * Mat4::from_translation(attachment.position())
                * Mat4::from_scale(Vec3::splat(*rider_scale));
            for event in &self.triggered_events[first..] {
                if let Some(request) =
                    callback(event, animation, &body_source.model, body_transform)
                {
                    self.unit_effects
                        .emit(request, &self.animations, now, random)?;
                }
            }
            Ok(())
        };
        let mut completed = |playback: &mut M2Playback,
                             key: i32,
                             animation: u16,
                             _: u32,
                             random: &mut CrtRand| {
            if let Some((owner, ..)) = &body {
                owner.complete_mount_animation(&source.model, playback, key, animation, random)?;
            }
            Ok(())
        };
        placement.passenger_playback_advance =
            Some(playback.borrow_mut().clock_with_bone_callbacks(
                &source.model,
                now as u32,
                random,
                Some(&mut completed),
                Some(&mut event),
            )?);
        Ok(())
    }
}
