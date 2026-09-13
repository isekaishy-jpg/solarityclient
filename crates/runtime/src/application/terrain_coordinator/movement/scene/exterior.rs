//! Publishes regular and distant terrain through the primary true-exterior bank.

use std::sync::Arc;

use glam::Vec3;
use solarity_rendering::{
    TerrainLowDetailMap, WorldCameraError, WorldCameraFrame, WorldFrustum, WorldHorizonScale,
    WorldLowDetailFrame,
};

use super::super::RuntimeTerrainCoordinator;
use super::WorldSceneAdmission;

impl WorldSceneAdmission {
    /// 79A790 applies 790AF0's exterior window before 799D40 selects ADT chunks.
    pub(super) fn terrain_frustum(
        &self,
        camera: WorldCameraFrame,
    ) -> Result<Option<WorldFrustum>, WorldCameraError> {
        self.outdoor_window
            .map(|window| WorldFrustum::new(camera, window))
            .transpose()
    }

    /// 79A870 reaches 791980 only through 79A790, independently of visible sky.
    pub(super) fn horizon_frame<'a>(
        &self,
        map: &'a Arc<TerrainLowDetailMap>,
        camera: WorldCameraFrame,
        fog_color: Vec3,
        scale: WorldHorizonScale,
    ) -> Result<Option<WorldLowDetailFrame<'a>>, WorldCameraError> {
        self.outdoor_window
            .map(|window| WorldLowDetailFrame::new(map, camera, fog_color, scale, window))
            .transpose()
    }
}

impl RuntimeTerrainCoordinator {
    /// Explicit offline captures inspect registration after presentation; normal
    /// gameplay never calls this additional query or emits these diagnostics.
    pub(in crate::application) fn log_captured_camera_scene(
        &mut self,
        camera: WorldCameraFrame,
        phase: &str,
        frame: usize,
    ) -> Result<(), crate::application::RuntimeMovementRegistrationError> {
        if let Some(active) = &mut self.active {
            let source = camera.camera();
            let eye = source.position();
            let registration = active.camera_registration(eye)?;
            let primary = registration.map(|value| {
                (
                    active.movement.roots[value.primary.owner].owner(),
                    value.primary.group,
                    value.primary.secondary_group,
                )
            });
            tracing::info!(phase, frame, ?eye, ?primary,
                target = ?source.target(), forward = ?camera.forward(), up = ?camera.up(),
                vertical_fov = ?source.vertical_field_of_view_radians(),
                near_clip = source.near_clip(), far_clip = source.far_clip(),
                aspect_ratio = camera.aspect_ratio(),
                exterior = ?active.movement.scene.outdoor_window,
                "captured World camera admission");
        }
        Ok(())
    }

    /// Reuses this frame's primary exterior admission for ADT surfaces and effects.
    pub(in crate::application) fn world_terrain_frustum(
        &self,
        camera: WorldCameraFrame,
    ) -> Result<Option<WorldFrustum>, WorldCameraError> {
        self.active.as_ref().map_or(Ok(None), |active| {
            active.movement.scene.terrain_frustum(camera)
        })
    }

    /// Borrows the map only when primary camera traversal admits exterior terrain.
    pub(in crate::application) fn world_low_detail_frame(
        &self,
        camera: WorldCameraFrame,
        fog_color: Vec3,
        scale: WorldHorizonScale,
    ) -> Result<Option<WorldLowDetailFrame<'_>>, WorldCameraError> {
        let Some(active) = &self.active else {
            return Ok(None);
        };
        let Some(map) = self.low_detail() else {
            return Ok(None);
        };
        active
            .movement
            .scene
            .horizon_frame(map, camera, fog_color, scale)
    }
}
