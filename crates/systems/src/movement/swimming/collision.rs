//! Native 760B40 sliding against ordinary geometry and reversed liquid planes.

use glam::Vec3;
use thiserror::Error;

use crate::collision::{MovementCollisionTriangle, MovementCollisionVolume, MovementSweepError};
use crate::movement::MovementGeometry;

const EPSILON: f32 = f32::from_bits(0x3580_0000);
const SEPARATION: f32 = f32::from_bits(0x3a83_126f);

/// Complete ordinary and water-surface candidates in the mover's coordinate space.
pub trait MovementSwimGeometry: MovementGeometry {
    /// The 75FF90 secondary bank preserves vertices and reverses liquid normals.
    fn water_triangles(&self) -> &[MovementCollisionTriangle];
}

/// One collected 3D movement interval; passenger detachment precedes this boundary.
#[derive(Clone, Copy, Debug)]
pub struct MovementSwimInterval {
    /// Foot position in the geometry's coordinate space.
    pub position: Vec3,
    /// Native body radius.
    pub radius: f32,
    /// Full body height; liquid contact uses three quarters of this height.
    pub height: f32,
    /// Requested interval duration on the unsigned movement clock.
    pub duration_ms: u32,
    /// Requested nonnegative travel distance.
    pub distance: f32,
    /// Normalized full 3D direction from the trajectory owner.
    pub direction: Vec3,
    /// Native ascent bit requests a jump on reaching the liquid boundary.
    pub ascending: bool,
}

/// Native swimming continuation and actions for the owning movement controller.
#[derive(Clone, Copy, Debug)]
pub struct MovementSwimAdvance {
    /// Position after all completed probes, including partial progress on failure.
    pub position: Vec3,
    /// The native loop consumes the full interval even when a provider fails.
    pub consumed_ms: u32,
    /// Rebuild the analytic anchor after a slide or exhausted contact loop.
    pub reset_motion_anchor: bool,
    /// Invoke the native swim-jump admission after an ascending surface contact.
    pub attempt_surface_jump: bool,
    /// The provider failed to supply complete geometry for a requested sweep.
    pub geometry_unavailable: bool,
}

/// Invalid interval state or geometry at the swimming response boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MovementSwimAdvanceError {
    /// Invalid input or non-finite response arithmetic.
    #[error("swim interval requires finite state and nonnegative distance")]
    InvalidState,
    /// The collision volume or candidate geometry could not be admitted.
    #[error(transparent)]
    Sweep(#[from] MovementSweepError),
}

impl MovementSwimInterval {
    /// Runs 760B40 with separate ordinary and water candidate banks.
    ///
    /// The caller must establish complete initial coverage and detach any
    /// movement parent first. Each non-tiny sweep can refresh that coverage.
    ///
    /// # Errors
    /// Rejects invalid state, volume, or non-finite collision arithmetic.
    pub fn advance<G: MovementSwimGeometry + ?Sized>(
        self,
        geometry: &mut G,
    ) -> Result<MovementSwimAdvance, MovementSwimAdvanceError> {
        if !self.direction.is_finite() || !self.distance.is_finite() || self.distance < 0. {
            return Err(MovementSwimAdvanceError::InvalidState);
        }
        MovementCollisionVolume::new(self.position, self.radius, self.height)?;
        let mut result = MovementSwimAdvance {
            position: self.position,
            consumed_ms: self.duration_ms,
            reset_motion_anchor: false,
            attempt_surface_jump: false,
            geometry_unavailable: false,
        };
        if self.duration_ms == 0 || self.distance.abs() < EPSILON {
            return Ok(result);
        }
        let total = self.duration_ms as f32;
        let mut elapsed = 0f32;
        let mut tiny_progress = 0;
        let mut ordinary_contact = false;
        let mut reanchor = false;
        let mut direction = self.direction;
        let mut distance = self.distance;
        loop {
            let volume = MovementCollisionVolume::new(result.position, self.radius, self.height)?;
            if !geometry.prepare_sweep(&volume, direction, distance) {
                result.geometry_unavailable = true;
                return Ok(result);
            }
            let mut sweep = volume.sweep_along(direction, distance, geometry.triangles())?;
            let mut normal = sweep
                .last_triangle()
                .map(|i| geometry.triangles()[i].normal());
            if normal.is_some() {
                ordinary_contact = true;
            } else if !ordinary_contact {
                let volume =
                    MovementCollisionVolume::new(result.position, self.radius, self.height * 0.75)?;
                if !geometry.prepare_sweep(&volume, direction, distance) {
                    result.geometry_unavailable = true;
                    return Ok(result);
                }
                sweep = volume.sweep_along(direction, distance, geometry.water_triangles())?;
                normal = sweep
                    .last_triangle()
                    .map(|i| geometry.water_triangles()[i].normal());
            }
            let allowed = f64::from(sweep.distance());
            let Some(normal) = normal else {
                // 482970 receives the displacement after three float stores.
                let delta = (direction.as_dvec3() * allowed).as_vec3();
                result.position += delta;
                result.reset_motion_anchor = reanchor;
                return Ok(result);
            };
            result.position =
                (result.position.as_dvec3() + direction.as_dvec3() * allowed).as_vec3();
            let progress =
                (allowed / f64::from(distance)) * (f64::from(total) - f64::from(elapsed));
            let remaining = f64::from(distance) - allowed;
            if progress + f64::from(EPSILON) <= 1. {
                tiny_progress += 1;
                if tiny_progress > 5 {
                    result.reset_motion_anchor = true;
                    return Ok(result);
                }
            } else {
                tiny_progress = 1;
                elapsed = (f64::from(elapsed) + progress) as f32;
            }
            if !ordinary_contact && self.ascending {
                result.attempt_surface_jump = true;
                result.reset_motion_anchor = reanchor || (total - elapsed).abs() >= EPSILON;
                return Ok(result);
            }
            if total - elapsed < 1. {
                result.reset_motion_anchor = reanchor || (total - elapsed).abs() >= EPSILON;
                return Ok(result);
            }
            reanchor = true;
            let normal = normal.as_dvec3();
            let direction_extended = direction.as_dvec3();
            // Native dot order is X, Z, Y and the separation survives projection.
            let correction = ((-normal.x * direction_extended.x - normal.z * direction_extended.z)
                - normal.y * direction_extended.y)
                * remaining
                + f64::from(SEPARATION);
            let projected = direction_extended * remaining + normal * correction;
            // 760E66--760EE8 stores all components but retains extended X and
            // the complete squared length through normalization.
            let length = ((projected.x * projected.x + projected.z * projected.z)
                + projected.y * projected.y)
                .sqrt();
            distance = length as f32;
            if !distance.is_finite() || !projected.is_finite() {
                return Err(MovementSwimAdvanceError::InvalidState);
            }
            if distance.abs() < EPSILON {
                result.reset_motion_anchor = true;
                return Ok(result);
            }
            direction = Vec3::new(
                (projected.x / length) as f32,
                (f64::from(projected.y as f32) / length) as f32,
                (f64::from(projected.z as f32) / length) as f32,
            );
        }
    }
}
