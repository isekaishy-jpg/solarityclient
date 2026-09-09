//! Outdoor unit scene admission shared with the next movement service pass.

use glam::Vec3;
use solarity_systems::WorldSceneDepthFrame;

use super::{RuntimeMovementRegistrationError, RuntimeTerrainCoordinator};

impl RuntimeTerrainCoordinator {
    /// 79A870 always traverses outdoor depth lists when the camera has no WMO.
    /// Cameras inside a WMO require the separate portal/exterior-window pass.
    pub(in crate::application) fn outdoor_unit_scene_frame(
        &mut self,
        eye: Vec3,
        target: Vec3,
    ) -> Result<Option<WorldSceneDepthFrame>, RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(None);
        };
        if active.camera_registration(eye)?.is_some() {
            return Ok(None);
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
