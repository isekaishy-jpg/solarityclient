//! One fall contact and its consumed time, from `0x00760FC0` / `0x0075EB00`.

use glam::{Vec2, Vec3};
use thiserror::Error;

use super::{MovementFallCrossing, MovementFallError, MovementFallTrajectory};
use crate::collision::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementFallContactKind,
    MovementSupportProfile, MovementSweepError,
};

// Native horizontal-distance admission threshold at 0x009F1224.
const DEGENERATE_TOLERANCE: f32 = f32::from_bits(0x3580_0000);

/// Fall state sampled at the start of one collision interval.
#[derive(Clone, Copy, Debug)]
pub struct MovementFallContactQuery {
    /// Resolved terminal mode and signed downward launch speed.
    pub trajectory: MovementFallTrajectory,
    /// World Z where this fall's current analytic curve began.
    pub launch_height: f32,
    /// Seconds elapsed on the fall curve before this interval.
    pub elapsed_seconds: f32,
    /// Seconds available to this displacement.
    pub interval_seconds: f32,
    /// Current horizontal fall speed after any preceding contact response.
    pub horizontal_speed: f32,
    /// Unit-control policy resolved by the movement owner.
    pub support_profile: MovementSupportProfile,
}

/// A collision result ready for the fall interval owner to apply.
#[derive(Clone, Copy, Debug)]
pub struct MovementFallContact {
    /// Displacement after sweep clamping and contact-time correction.
    pub displacement: Vec3,
    /// Allowed distance, adjusted when contact time shortens horizontal travel.
    pub distance: f32,
    /// Add this to the remaining displacement's XY components for a slide.
    pub horizontal_correction: Vec2,
    /// Time consumed before the owner applies the continuation decision.
    pub consumed_seconds: f32,
    /// Landing, ceiling reset, slide, or clear travel.
    pub kind: MovementFallContactKind,
    /// Selected triangle in the caller's ordered candidate slice.
    pub last_triangle: Option<usize>,
}

/// Invalid input or non-finite result at the fall contact boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MovementFallContactError {
    /// Clock, height, or horizontal speed is not an admitted finite scalar.
    #[error("movement fall contact state is invalid")]
    InvalidState,
    /// The collision query rejected its geometry or displacement.
    #[error(transparent)]
    Sweep(#[from] MovementSweepError),
    /// The analytic curve rejected the contact height or time.
    #[error(transparent)]
    Trajectory(#[from] MovementFallError),
    /// Arithmetic cannot represent the resolved contact in native float fields.
    #[error("movement fall contact result is not finite")]
    NonFiniteResult,
}

impl MovementFallContactQuery {
    /// Sweeps an already-generated fall displacement, resolves landing/ceiling
    /// policy, and computes the contact time and remaining horizontal response.
    ///
    /// The caller supplies complete ordered candidates in the body's space.
    /// This is one iteration of the native fall integrator; applying position,
    /// repeating slides, resetting fall clocks, and sending events belong to
    /// the movement interval owner. Candidate identities remain indexed by the
    /// returned triangle. The query allocates no memory or persistent state.
    ///
    /// # Errors
    /// Returns [`MovementFallContactError`] for invalid state, travel, or an
    /// unrepresentable result. No caller state is mutated on failure.
    pub fn resolve(
        self,
        volume: &MovementCollisionVolume,
        displacement: Vec3,
        triangles: &[MovementCollisionTriangle],
    ) -> Result<MovementFallContact, MovementFallContactError> {
        self.validate()?;
        let geometry = volume.fall_contact_geometry(
            displacement,
            triangles,
            self.trajectory.apex_seconds() - f64::from(self.elapsed_seconds),
            self.horizontal_speed,
            self.support_profile,
        )?;
        let mut result = MovementFallContact {
            displacement: geometry.displacement,
            distance: geometry.distance,
            horizontal_correction: geometry.horizontal_correction,
            consumed_seconds: self.interval_seconds,
            kind: geometry.kind,
            last_triangle: geometry.last_triangle,
        };
        if result.kind != MovementFallContactKind::Clear {
            self.resolve_time(
                volume.foot_origin().z,
                displacement.truncate().as_dvec2().length() as f32,
                &mut result,
            )?;
        }
        if !result.displacement.is_finite()
            || !result.distance.is_finite()
            || !result.horizontal_correction.is_finite()
            || !result.consumed_seconds.is_finite()
        {
            return Err(MovementFallContactError::NonFiniteResult);
        }
        Ok(result)
    }
}

impl MovementFallContactQuery {
    /// Admit state once before any geometric work.
    fn validate(self) -> Result<(), MovementFallContactError> {
        if !self.launch_height.is_finite()
            || [
                self.elapsed_seconds,
                self.interval_seconds,
                self.horizontal_speed,
            ]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(MovementFallContactError::InvalidState);
        }
        Ok(())
    }

    /// Native early-root policy `0x0075E040`, then `0x0075EB00` time/XY clamping.
    fn resolve_time(
        self,
        height: f32,
        horizontal_distance: f32,
        result: &mut MovementFallContact,
    ) -> Result<(), MovementFallContactError> {
        let elapsed = f64::from(self.elapsed_seconds);
        let interval = f64::from(self.interval_seconds);
        let apex = self.trajectory.apex_seconds();
        let crossing = if result.kind == MovementFallContactKind::Ceiling
            || (apex >= 0.0
                && (apex > elapsed + interval
                    || (apex >= elapsed
                        && ((apex - elapsed) * f64::from(self.horizontal_speed)).powi(2)
                            > result.displacement.truncate().as_dvec2().length_squared())))
        {
            MovementFallCrossing::Ascending
        } else {
            MovementFallCrossing::Descending
        };
        let horizontal_time = if horizontal_distance > DEGENERATE_TOLERANCE {
            (result.displacement.truncate().as_dvec2().length() / f64::from(horizontal_distance)
                * interval) as f32
        } else {
            0.0
        };
        let distance = (f64::from(self.launch_height)
            - f64::from(height)
            - f64::from(result.displacement.z)) as f32;
        let root = self
            .trajectory
            .seconds_at_distance_extended(distance, crossing)?;
        if root <= elapsed {
            result.displacement = Vec3::ZERO;
            result.distance = 0.0;
            result.consumed_seconds = 0.0;
        } else if root - elapsed <= interval {
            let consumed = root - elapsed;
            result.consumed_seconds = consumed as f32;
            if consumed < f64::from(horizontal_time) {
                let factor = consumed / f64::from(horizontal_time);
                let delta = result.displacement.as_dvec3();
                let corrected = glam::DVec3::new(delta.x * factor, delta.y * factor, delta.z);
                result.displacement = corrected.as_vec3();
                result.distance = corrected.length() as f32;
            }
        }
        Ok(())
    }
}
