//! Per-sweep world collection fallback at 0x0075F0A0.

use glam::Vec3;

use super::sweep::sweep_extrusion;
use super::{MovementCollisionVolume, MovementSweepError};
use crate::collision::MovementCollisionBounds;

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
}
