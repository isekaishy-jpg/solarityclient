//! Native 0x0075FF90 collection bounds covering an interval's private probes.

use glam::Vec3;
use thiserror::Error;

use super::{MovementFallError, MovementFallTrajectory, MovementGroundProfile};
use crate::collision::{MovementCollectionError, MovementCollisionBounds};

const CONTACT_TOLERANCE: f32 = f32::from_bits(0x3ab6_0b61);
const SLOPE_EXPANSION: f32 = f32::from_bits(0x3f98_8b62);
const RADIAL_EXPANSION: f32 = f32::from_bits(0x3fb5_04f3);
const FALL_EXPANSION: f32 = f32::from_bits(0xbf97_e4ad);

/// Resolved collection branch, separate from movement response and wire flags.
#[derive(Clone, Copy, Debug)]
pub enum MovementIntervalMode {
    /// Surface following, step probes, and speculative falling.
    Grounded(MovementGroundProfile),
    /// Analytic fall at the end of the requested interval.
    Airborne {
        /// Unsigned retained fall clock before this interval.
        fall_time_ms: u32,
        /// Original launch Z in the same coordinate space as the foot origin.
        launch_height: f32,
        /// Current normal/slow fall curve and signed downward launch speed.
        trajectory: MovementFallTrajectory,
    },
    /// Bounds for native swimming/flying queries; their response owners are separate.
    SwimmingOrFlying,
}

/// Inputs to interval collection after movement-space resolution.
#[derive(Clone, Copy, Debug)]
pub struct MovementIntervalRequest {
    /// Foot origin, in the coordinate space passed to world collection.
    pub position: Vec3,
    /// Resolved nonnegative collision radius.
    pub radius: f32,
    /// Resolved nonnegative effective collision height.
    pub height: f32,
    /// Native separate nonnegative requested distance image.
    pub distance: f32,
    /// Resolved direction; retained without renormalization.
    pub direction: Vec3,
    /// Remaining unsigned interval duration.
    pub duration_ms: u32,
    /// Resolved ground, falling, or swimming/flying collection policy.
    pub mode: MovementIntervalMode,
}

/// Original body bounds and expanded candidate bounds passed to `0x00783910`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementIntervalBounds {
    body: MovementCollisionBounds,
    query: MovementCollisionBounds,
}

impl MovementIntervalBounds {
    /// Returns the unexpanded body bounds used by native residency policy.
    #[must_use]
    pub const fn body(self) -> MovementCollisionBounds {
        self.body
    }

    /// Returns the full candidate box, including step/fall probes and tolerance.
    #[must_use]
    pub const fn query(self) -> MovementCollisionBounds {
        self.query
    }
}

/// Invalid interval input, trajectory, or unrepresentable collection bounds.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MovementIntervalBoundsError {
    /// A scalar is non-finite or a dimension, distance, or step height is negative.
    #[error("movement interval bounds input is invalid")]
    InvalidInput,
    /// The resulting native float bounds are invalid.
    #[error(transparent)]
    Bounds(#[from] MovementCollectionError),
    /// The analytic fall cannot be represented.
    #[error(transparent)]
    Fall(#[from] MovementFallError),
}

impl MovementIntervalRequest {
    /// Builds stock's collection box, including zero-distance step/fall probes.
    ///
    /// Coordinate conversion belongs to the movement/transport owner. Collect
    /// every candidate in `query()` before advancing any interval response.
    ///
    /// # Errors
    /// Returns an error for invalid inputs or non-finite generated bounds.
    pub fn collection_bounds(self) -> Result<MovementIntervalBounds, MovementIntervalBoundsError> {
        if !self.position.is_finite()
            || !self.direction.is_finite()
            || [self.radius, self.height, self.distance]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(MovementIntervalBoundsError::InvalidInput);
        }
        let body = MovementCollisionBounds::new(
            self.position - Vec3::new(self.radius, self.radius, 0.),
            self.position + Vec3::new(self.radius, self.radius, self.height),
        )?;
        let mut minimum = body.minimum();
        let mut maximum = body.maximum();
        let distance = f64::from(self.distance);
        let direction = self.direction.as_dvec3();
        match self.mode {
            MovementIntervalMode::Grounded(profile) => {
                let step = match profile {
                    MovementGroundProfile::PlayerControlled { step_height } => step_height,
                    MovementGroundProfile::Other => 2.,
                };
                if !step.is_finite() || step < 0. {
                    return Err(MovementIntervalBoundsError::InvalidInput);
                }
                let step = f64::from(step);
                let reach = f64::from(self.radius + CONTACT_TOLERANCE)
                    .max(step * f64::from(SLOPE_EXPANSION))
                    + distance;
                minimum = minimum.min((body.minimum().as_dvec3() + direction * reach).as_vec3());
                maximum = maximum.max((body.maximum().as_dvec3() + direction * reach).as_vec3());
                let half = distance * 0.5;
                let center = self.position.as_dvec3() + direction * half;
                // Native keeps X in x87 but reloads rounded Y and Z.
                let x = center.x;
                let y = f64::from(center.y as f32);
                let z = center.z as f32;
                let extent = f64::from(self.radius) * f64::from(RADIAL_EXPANSION) + half;
                minimum = minimum.min(Vec3::new((x - extent) as f32, (y - extent) as f32, z));
                maximum = maximum.max(Vec3::new((x + extent) as f32, (y + extent) as f32, z));
                maximum.z = (f64::from(maximum.z) + distance.max(step * 2.)) as f32;
                minimum.z =
                    (f64::from(minimum.z) - (distance * f64::from(SLOPE_EXPANSION) + step)) as f32;
            }
            MovementIntervalMode::Airborne {
                fall_time_ms,
                launch_height,
                trajectory,
            } => {
                if !launch_height.is_finite() {
                    return Err(MovementIntervalBoundsError::InvalidInput);
                }
                // 9870D0 returns current-Z minus launch-Z plus the absolute
                // curve sample. Preserve that extended result through subtraction.
                let fall = trajectory
                    .distance_at_millis_extended(fall_time_ms.wrapping_add(self.duration_ms))?
                    + (f64::from(self.position.z) - f64::from(launch_height));
                let displacement = (direction * distance).as_vec3();
                let vertical = f64::from(displacement.z) - fall;
                let displacement = glam::DVec3::new(
                    f64::from(displacement.x),
                    f64::from(displacement.y),
                    vertical,
                );
                minimum = minimum.min((body.minimum().as_dvec3() + displacement).as_vec3());
                maximum = maximum.max((body.maximum().as_dvec3() + displacement).as_vec3());
                let extra = f64::from(vertical as f32) * f64::from(FALL_EXPANSION);
                if extra > 0. {
                    minimum.x = (f64::from(minimum.x) - extra) as f32;
                    minimum.y = (f64::from(minimum.y) - extra) as f32;
                    maximum.x = (f64::from(maximum.x) + extra) as f32;
                    maximum.y = (f64::from(maximum.y) + extra) as f32;
                }
            }
            MovementIntervalMode::SwimmingOrFlying => {
                let half = distance * 0.5;
                let center = self.position.as_dvec3() + direction * half;
                let x = center.x;
                let y = f64::from(center.y as f32);
                let z = f64::from(center.z as f32);
                let radius = f64::from(self.radius) * f64::from(RADIAL_EXPANSION);
                minimum = minimum.min(Vec3::new(
                    ((x - half) - radius) as f32,
                    ((y - half) - radius) as f32,
                    (z - half) as f32,
                ));
                maximum = maximum.max(Vec3::new(
                    ((x + half) + radius) as f32,
                    ((y + half) + radius) as f32,
                    ((z + half) + radius.max(f64::from(self.height))) as f32,
                ));
            }
        }
        let query = MovementCollisionBounds::new(
            minimum - Vec3::splat(CONTACT_TOLERANCE),
            maximum + Vec3::splat(CONTACT_TOLERANCE),
        )?;
        Ok(MovementIntervalBounds { body, query })
    }
}
