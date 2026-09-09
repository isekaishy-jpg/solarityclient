//! Outdoor unit scene admission shared with the next movement service pass.

use glam::Vec3;
use solarity_rendering::WorldCameraFrame;
use solarity_systems::{WorldModelVisibilityError, WorldSceneCameraFrame, WorldSceneDepthFrame};

use super::{RuntimeMovementRegistrationError, RuntimeTerrainCoordinator};

impl RuntimeTerrainCoordinator {
    /// 79A870 always traverses outdoor depth lists when the camera has no WMO.
    /// With an interior camera, 79A870 runs those same lists only after a true
    /// exterior portal survives the primary root's camera-group traversal.
    pub(in crate::application) fn outdoor_unit_scene_frame(
        &mut self,
        camera: WorldCameraFrame,
    ) -> Result<Option<WorldSceneDepthFrame>, RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(None);
        };
        let source = camera.camera();
        let eye = source.position();
        let target = source.target();
        if let Some(registration) = active.camera_registration(eye)? {
            let scene = WorldSceneCameraFrame::perspective(
                eye,
                target,
                source.view_direction(),
                source.up(),
                source
                    .vertical_field_of_view_radians()
                    .ok_or(WorldModelVisibilityError::InvalidCameraProjection)?,
                camera.aspect_ratio(),
                [source.near_clip(), source.far_clip()],
            )?;
            // Secondary-root group visits remain relevant to indoor units,
            // but 79A870 resets their windows before the primary root runs.
            let registration = registration.primary;
            let initial = [
                registration.group,
                registration.secondary_group.unwrap_or(registration.group),
            ];
            let initial_count = 1 + usize::from(registration.secondary_group.is_some());
            let mut query = std::mem::take(&mut active.movement.exterior_scene_query);
            let admitted = (|| {
                let root = active.movement.roots[registration.owner];
                let root = active.registration_root_mut(root)?;
                Ok::<_, RuntimeMovementRegistrationError>(query.query_camera_root(
                    root,
                    scene,
                    &initial[..initial_count],
                )?)
            })();
            // Return scratch even when a resident generation rejects a query.
            active.movement.exterior_scene_query = query;
            if !admitted? {
                return Ok(None);
            }
        }
        Ok(Some(WorldSceneDepthFrame::new(eye, target)?))
    }

    /// 7C2A70's selected interior links the unit into WMO group lists instead.
    pub(in crate::application) fn unit_scene_is_interior(
        &mut self,
        position: Vec3,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(false);
        };
        Ok(active
            .unit_registration(position)?
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior()))
    }
}
