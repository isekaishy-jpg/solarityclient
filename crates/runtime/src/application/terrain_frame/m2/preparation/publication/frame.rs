//! Native event, light and attachment consumers share the selected CPU bone view.

use super::super::super::{
    M2GpuPlacement, M2GpuPlacementOwner, RuntimeTerrainFrameError, append_triggered_events,
    placement_mesh_color, sample_mount_camera,
};
use super::{CpuPublication, CpuSample};
use solarity_asset::DecodedM2Model;
use solarity_rendering::{M2BoneTransforms, sample_m2_lights_into};
use std::rc::Rc;

impl CpuPublication<'_> {
    pub(in crate::application::terrain_frame::m2::preparation) fn publish(
        self,
        model: &DecodedM2Model,
        placement: &M2GpuPlacement,
        bone_pose: &dyn M2BoneTransforms,
        sample: CpuSample,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile_scope = solarity_profiling::detail_profile!(
            "runtime.application.terrain_frame.m2.preparation.publication.frame.publish"
        );
        let CpuSample {
            clock,
            event_window,
            animation_time_ms,
            placement_opacity,
            publishes_lights,
        } = sample;
        self.retirement
            .publish_attachments(placement, model, bone_pose, clock)?;
        let first_event = self.triggered_events.len();
        append_triggered_events(
            self.triggered_events,
            model,
            placement.owner,
            placement.transform,
            &placement.sound_lifetime,
            bone_pose,
            event_window,
        )?;
        if let Some(effect) = &placement.unit_effect {
            effect.bind_sound_events(&mut self.triggered_events[first_event..]);
        }
        if let Some(animation) = &placement.unit_animation {
            self.unit_effects
                .update_anchor(animation, model, bone_pose, placement.transform)?;
        }
        if publishes_lights {
            solarity_rendering::sample_m2_scene_lights_into(
                model.animations(),
                bone_pose,
                clock,
                placement.transform,
                &mut self.scene_lighting.sample_directional,
                &mut self.scene_lighting.sample_points,
            )?;
            self.scene_lighting.publish(
                placement.light_lifetime.get_or_init(|| Rc::new(())),
                placement_mesh_color(placement.owner, placement.color).w * placement_opacity,
            )?;
        }
        if matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }) {
            sample_m2_lights_into(
                model.animations(),
                bone_pose,
                clock,
                placement.transform,
                self.glue_directional_lights,
                self.glue_point_lights,
            )?;
            for attachment_id in self.glue_attachment_ids {
                let attachment = model.attachment(*attachment_id).ok_or_else(|| {
                    RuntimeTerrainFrameError::MissingGlueM2Attachment {
                        model: model.path().clone(),
                        attachment_id: *attachment_id,
                    }
                })?;
                let transform = bone_pose.attachment_transform(
                    model.animations(),
                    attachment,
                    clock,
                    placement.transform,
                )?;
                self.glue_attachment_transforms
                    .push((*attachment_id, transform));
            }
        }
        if let M2GpuPlacementOwner::PlayerMount { guid }
        | M2GpuPlacementOwner::RemotePlayerMount { guid }
        | M2GpuPlacementOwner::CreatureMount { guid } = placement.owner
        {
            if matches!(placement.owner, M2GpuPlacementOwner::PlayerMount { .. }) {
                *self.mount_camera_sample = Some(sample_mount_camera(
                    model,
                    placement.transform,
                    bone_pose,
                    animation_time_ms,
                )?);
            }
            let attachment = model.attachment(0).ok_or_else(|| {
                RuntimeTerrainFrameError::MissingMountM2Attachment {
                    model: model.path().clone(),
                    attachment_id: 0,
                }
            })?;
            let transform = bone_pose.attachment_transform(
                model.animations(),
                attachment,
                clock,
                placement.transform,
            )?;
            self.rider_transforms.push((guid, transform));
        }
        if let M2GpuPlacementOwner::PlayerBody { guid }
        | M2GpuPlacementOwner::RemotePlayerBody { guid }
        | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
        {
            for (_owner_guid, point) in self
                .requested_items
                .iter()
                .filter(|(owner_guid, _point)| *owner_guid == guid)
            {
                let attachment = model.attachment(point.id()).ok_or_else(|| {
                    RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                        model: model.path().clone(),
                        attachment_id: point.id(),
                    }
                })?;
                let transform = bone_pose.attachment_transform(
                    model.animations(),
                    attachment,
                    clock,
                    placement.transform,
                )?;
                self.item_transforms.push((guid, *point, transform));
            }
        }
        if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
            for (_owner_guid, _owner_item_point, effect_point) in self
                .requested_visuals
                .iter()
                .filter(|(owner_guid, item_point, _)| *owner_guid == guid && *item_point == point)
            {
                let attachment = model.attachment(*effect_point).ok_or_else(|| {
                    RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                        model: model.path().clone(),
                        attachment_id: *effect_point,
                    }
                })?;
                let transform = bone_pose.attachment_transform(
                    model.animations(),
                    attachment,
                    clock,
                    placement.transform,
                )?;
                self.visual_transforms
                    .push((guid, point, *effect_point, transform));
            }
        }
        Ok(())
    }
}
