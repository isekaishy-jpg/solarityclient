//! Publishes distant terrain through the primary true-exterior scene bank.

use std::sync::Arc;

use glam::Vec3;
use solarity_rendering::{
    TerrainLowDetailMap, WorldCameraError, WorldCameraFrame, WorldHorizonScale, WorldLowDetailFrame,
};

use super::super::RuntimeTerrainCoordinator;
use super::WorldSceneAdmission;

impl WorldSceneAdmission {
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
