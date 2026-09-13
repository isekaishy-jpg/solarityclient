//! Retained local appearance still observes every native motion and camera update.

use super::super::{
    RuntimePlayerError, RuntimePlayerPoll, RuntimePlayerPresentation, resolve_resident_animation,
};
use solarity_ecs::ActiveWorld;
use solarity_systems::{
    UnitLocomotionAnimation, resolve_mounted_player_camera_pose, resolve_unit_locomotion_animation,
};

impl RuntimePlayerPresentation {
    /// Advances independent motion, animation and camera state for retained appearance.
    pub(super) fn update_local_pose(
        &mut self,
        world: &ActiveWorld,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        let guid = world.local_player_guid()?;
        let Some(presentation) = world.local_player_presentation() else {
            return Ok(RuntimePlayerPoll::Pending);
        };
        let animation_tier = presentation.animation_tier();
        let transform = world.local_player_transform()?;
        let view = world.local_player_view()?;
        let requested_animation = world.movement_state(guid).map_or(
            UnitLocomotionAnimation::STAND,
            resolve_unit_locomotion_animation,
        );
        if let Some(resident) = self.resident.as_mut() {
            resident.world_transform = transform;
            resident.view = view;
            resident.animation = resolve_resident_animation(
                &self.animations,
                &resident.model,
                if resident.mount.is_some() {
                    UnitLocomotionAnimation::MOUNT
                } else {
                    requested_animation
                },
                animation_tier,
            )?;
            if let Some(mount) = resident.mount.as_mut() {
                mount.animation = resolve_resident_animation(
                    &self.animations,
                    &mount.model,
                    requested_animation,
                    animation_tier,
                )?;
            }
            let camera_heights = resident
                .camera_height_state
                .sample(resident.camera_time_ms)?;
            resident.camera_height = camera_heights.subject_height();
            resident.camera_pose = Some(resolve_mounted_player_camera_pose(
                transform,
                view,
                camera_heights,
            )?);
        }
        self.synchronize_local_animation(world)?;
        Ok(RuntimePlayerPoll::Current)
    }
}
