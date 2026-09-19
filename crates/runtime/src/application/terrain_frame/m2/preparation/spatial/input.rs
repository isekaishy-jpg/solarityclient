//! Value-only collector inputs; no animation, callback, resource or scene borrows.

use crate::application::m2_spatial::StaticM2Spatial;
use crate::application::terrain_frame::m2::RuntimeTerrainFrameError;
use crate::application::terrain_frame::shadow::{ModelShadowKind, WorldShadowAdmission};
use glam::Vec3;
use solarity_rendering::WorldFrustum;
use solarity_systems::MovementCollisionBounds;

/// These camera and collector volumes remain fixed throughout the owned phase.
#[derive(Clone, Copy)]
pub(in crate::application::terrain_frame::m2) struct SpatialView {
    pub camera: Vec3,
    pub detail: f32,
    pub frustum: WorldFrustum,
    pub shadows: Option<WorldShadowAdmission>,
}

/// Captured after WMO scene admission; each entry is independent of every other.
#[derive(Clone, Copy)]
pub(in crate::application::terrain_frame::m2) struct StaticAdmissionInput {
    pub spatial: Option<StaticM2Spatial>,
    pub shadow_kind: Option<ModelShadowKind>,
    pub shadow_membership: u8,
    pub publishes_lights: bool,
    pub doodad_active: bool,
    pub doodad_visible: bool,
    pub doodad_opacity: f32,
}

/// Only these pure decisions cross back into ordered placement traversal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::application::terrain_frame::m2) struct StaticAdmission {
    pub environment_maps: u8,
    pub scenery_opacity: f32,
    pub rejected: bool,
}

impl StaticAdmissionInput {
    /// Preserves admission.rs's shadow-first gates. In particular, camera/portal
    /// invisibility and ordinary fade never suppress an independent shadow caster
    /// or a light owner. Invalid bounds fail only if their collector consumes them.
    pub(super) fn evaluate(
        self,
        view: SpatialView,
    ) -> Result<StaticAdmission, RuntimeTerrainFrameError> {
        let mut maps = 0;
        if let (Some(shadows), Some(kind), Some(spatial)) =
            (view.shadows, self.shadow_kind, self.spatial)
            && spatial.scenery().admits_shadow(view.camera, view.detail)
        {
            let possible = shadows.model_maps(kind, spatial.sphere().1, shadows.active_maps());
            if possible != 0 {
                let (minimum, maximum) = spatial.world_bounds();
                let bounds = MovementCollisionBounds::new(minimum, maximum)
                    .map_err(crate::application::RuntimeMovementRegistrationError::from)?;
                maps = shadows.admitted_maps(bounds) & possible & self.shadow_membership;
            }
        }
        let opacity = if self.doodad_active {
            self.doodad_opacity
        } else {
            self.spatial.map_or(1., |spatial| {
                spatial.scenery().opacity(view.camera, view.detail)
            })
        };
        let mut rejected = false;
        if maps == 0 && !self.publishes_lights {
            rejected = !self.doodad_visible || opacity == 0.;
            if !rejected
                && !self.doodad_active
                && let Some(spatial) = self.spatial
            {
                let (center, radius) = spatial.sphere();
                rejected = !view.frustum.contains_sphere(center, radius)?;
            }
        }
        Ok(StaticAdmission {
            environment_maps: maps,
            scenery_opacity: opacity,
            rejected,
        })
    }
}
