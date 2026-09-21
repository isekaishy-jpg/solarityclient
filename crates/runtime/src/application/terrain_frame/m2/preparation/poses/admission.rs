//! Worker palette demand includes camera and inherited shadow consumers.

use glam::Mat4;
use solarity_rendering::{WorldCameraFrame, WorldFrustum, WorldShadowProjection};

use super::super::super::{
    M2GpuPlacement, M2GpuSource, RuntimeTerrainFrameError, placement_bounding_sphere, shadow,
};
use crate::application::terrain_frame::shadow::SceneryShadowQueries;

/// Immutable geometry admission for units whose placement is already final.
pub(in crate::application::terrain_frame::m2) struct PoseAdmission<'a> {
    pub(super) view: Mat4,
    camera: WorldCameraFrame,
    frustum: WorldFrustum,
    primary: Option<WorldShadowProjection>,
    scenery: Option<SceneryShadowQueries<'a>>,
    detail: f32,
}

impl<'a> PoseAdmission<'a> {
    /// Captures the same camera and shadow collectors used by final traversal.
    pub(in crate::application::terrain_frame::m2) fn new(
        camera: WorldCameraFrame,
        frustum: WorldFrustum,
        primary: Option<WorldShadowProjection>,
        scenery: Option<SceneryShadowQueries<'a>>,
        detail: f32,
    ) -> Self {
        Self {
            view: camera.view(),
            camera,
            frustum,
            primary,
            scenery,
            detail,
        }
    }

    /// Camera culling must not force admitted shadow palettes back onto the
    /// client thread. These are the ordinary native collector predicates.
    pub(super) fn allows(
        &self,
        source: &M2GpuSource,
        placement: &M2GpuPlacement,
        shadow_root: Option<(&M2GpuSource, &M2GpuPlacement)>,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let (center, radius) = placement_bounding_sphere(&source.model, placement.transform);
        if self.frustum.contains_sphere(center, radius)? {
            return Ok(true);
        }
        // Attached units inherit the root's shadow registration; their own
        // bounds may be outside the light even while the root is admitted.
        let Some((source, placement)) = shadow_root else {
            return Ok(false);
        };
        if let Some(projection) = self.primary
            && shadow::admits_root(
                projection,
                source,
                placement,
                self.scenery.map(|queries| queries.admission),
            )?
        {
            return Ok(true);
        }
        if let Some(queries) = self.scenery {
            return Ok(shadow::environment_maps(
                queries,
                source,
                placement,
                self.camera.camera().position(),
                self.detail,
            )? != 0);
        }
        Ok(false)
    }
}
