//! Per-sweep world collection fallback at 0x0075F0A0.

use glam::Vec3;

use super::sweep::sweep_extrusion;
use super::{MovementCollisionVolume, MovementSweepError};
use crate::collision::MovementCollisionBounds;
use crate::movement::MovementTransportFrame;

// Native 0x00A33B7C, applied only after an endpoint escapes the old box.
const REFRESH_PADDING: f32 = f32::from_bits(0x3e2a_aaab);

impl MovementCollisionVolume {
    /// Checks stock's translated body corners against a complete candidate cache.
    ///
    /// Returns `None` on an inclusive cache hit or a tiny sweep. On a miss, the
    /// returned box joins the previous region to the padded endpoint body. The
    /// caller must collect that entire region before running the narrow phase,
    /// and publish it as cached only after successful collection. Coordinates
    /// must already share the cache's world space; passenger conversion belongs
    /// to the transport owner. Direction/distance retain the ground caller's
    /// separate float images, including the native minimum extrusion length.
    ///
    /// # Errors
    /// Returns an error for invalid travel or non-finite generated bounds.
    pub fn sweep_refresh_bounds(
        &self,
        direction: Vec3,
        distance: f32,
        cached: MovementCollisionBounds,
    ) -> Result<Option<MovementCollisionBounds>, MovementSweepError> {
        let Some(extrusion) = sweep_extrusion(direction, distance)? else {
            return Ok(None);
        };
        let minimum =
            Vec3::new(self.vertices[5].x, self.vertices[5].y, self.vertices[0].z) + extrusion;
        let maximum = self.vertices[7] + extrusion;
        refresh_bounds(minimum, maximum, cached)
    }

    /// Runs native 75F0A0's world cache test for a passenger-space body and sweep.
    /// The parent's matrix transforms the foot and extrusion before the unchanged
    /// radius/height construct a world-axis box.
    ///
    /// # Errors
    /// Returns an error for invalid travel or non-finite generated world bounds.
    pub fn sweep_refresh_bounds_in_frame(
        &self,
        direction: Vec3,
        distance: f32,
        cached: MovementCollisionBounds,
        frame: MovementTransportFrame,
    ) -> Result<Option<MovementCollisionBounds>, MovementSweepError> {
        let Some(extrusion) = sweep_extrusion(direction, distance)? else {
            return Ok(None);
        };
        let extrusion = frame.world_direction(extrusion);
        let origin = frame.world_position(self.foot_origin());
        let minimum = origin - Vec3::new(self.radius, self.radius, 0.) + extrusion;
        let maximum = origin + Vec3::new(self.radius, self.radius, self.height) + extrusion;
        refresh_bounds(minimum, maximum, cached)
    }
}

/// Inclusive containment and padded union use world coordinates in both modes.
fn refresh_bounds(
    minimum: Vec3,
    maximum: Vec3,
    cached: MovementCollisionBounds,
) -> Result<Option<MovementCollisionBounds>, MovementSweepError> {
    if !minimum.is_finite() || !maximum.is_finite() {
        return Err(MovementSweepError::InvalidCollectionBounds);
    }
    if cached.minimum().cmple(minimum).all() && maximum.cmple(cached.maximum()).all() {
        return Ok(None);
    }
    let padding = Vec3::splat(REFRESH_PADDING);
    MovementCollisionBounds::new(
        (minimum - padding).min(cached.minimum()),
        (maximum + padding).max(cached.maximum()),
    )
    .map(Some)
    .map_err(|_| MovementSweepError::InvalidCollectionBounds)
}
