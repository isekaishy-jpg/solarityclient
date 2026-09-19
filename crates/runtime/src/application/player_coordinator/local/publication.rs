//! Current movement and camera state are applied only when owned resources publish.

use super::super::{
    ResidentPlayerModel, RuntimePlayerError, RuntimePlayerPoll, RuntimePlayerPresentation,
};
use solarity_ecs::ActiveWorld;
use solarity_systems::PlayerCameraHeightState;

impl RuntimePlayerPresentation {
    /// Camera continuity belongs to the current resident, never a worker snapshot.
    pub(super) fn publish_local(
        &mut self,
        world: &ActiveWorld,
        mut resident: ResidentPlayerModel,
        asynchronous: bool,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        let previous_camera = self.resident.as_ref().and_then(|previous| {
            (previous.path() == resident.path() && previous.object_scale == resident.object_scale)
                .then_some((
                    previous.camera_height_state,
                    previous.camera_time_ms,
                    previous.mount_key.clone(),
                ))
        });
        let (mut camera, time, previous_mount) = previous_camera.unwrap_or((
            PlayerCameraHeightState::new(resident.camera_height),
            0.0,
            None,
        ));
        if previous_mount != resident.mount_key {
            if resident.mount_key.is_some() {
                camera.begin_mount_generation(time)?;
            } else {
                camera.set_mounted(false, time)?;
            }
        }
        resident.camera_height_state = camera;
        resident.camera_time_ms = time;
        let previous = self.resident.replace(resident);
        if asynchronous {
            self.local_worker.retire(previous);
        }
        // Admission-time transforms/animation are never published over newer input.
        self.update_local_pose(world)?;
        self.textures.collect_unused();
        Ok(RuntimePlayerPoll::ModelLoaded)
    }
}
