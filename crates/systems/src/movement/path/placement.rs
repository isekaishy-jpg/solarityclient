//! Spline target decisions surrounding the native movement collision interval.

use glam::{DVec3, Vec3};

use super::MovementSplineError;

/// A sampled local spline target before collision changes the retained position.
///
/// The native pre-collision check (`6E9C30`) and post-collision correction
/// (`6E9470`) intentionally differ at the three-yard boundary.
#[derive(Clone, Copy, Debug)]
pub struct MovementSplineTarget {
    target: Vec3,
    delta: DVec3,
    duration_ms: u32,
}

impl MovementSplineTarget {
    /// Captures one nonempty movement interval in the owner's coordinate space.
    ///
    /// # Errors
    /// Rejects nonfinite positions or an empty interval.
    pub fn new(current: Vec3, target: Vec3, duration_ms: u32) -> Result<Self, MovementSplineError> {
        if !current.is_finite() || !target.is_finite() || duration_ms == 0 {
            return Err(MovementSplineError::InvalidDefinition);
        }
        Ok(Self {
            target,
            delta: target.as_dvec3() - current.as_dvec3(),
            duration_ms,
        })
    }

    /// `6E9C30` snaps and returns before collision for excessive displacement.
    #[must_use]
    pub fn requires_snap(self, movement_flags: u32) -> bool {
        if movement_flags & 0xc0100f == 0 {
            return false;
        }
        let horizontal = self.delta.x * self.delta.x + self.delta.y * self.delta.y;
        let seconds = f64::from(self.duration_ms) * f64::from(0.001_f32);
        horizontal / (seconds * seconds) > 3600.0 || horizontal + self.delta.z * self.delta.z > 9.0
    }

    /// Retains collision corrections below three yards while the path is active.
    ///
    /// Explicit forced placements belong to the spline's separate completion
    /// operation and bypass this conditional correction.
    ///
    /// # Errors
    /// Rejects a nonfinite collision result.
    pub fn corrected_position(self, actual: Vec3) -> Result<Vec3, MovementSplineError> {
        if !actual.is_finite() {
            return Err(MovementSplineError::InvalidDefinition);
        }
        let delta = self.target.as_dvec3() - actual.as_dvec3();
        Ok(
            if delta.x * delta.x + delta.y * delta.y + delta.z * delta.z >= 9.0 {
                self.target
            } else {
                actual
            },
        )
    }

    /// `75D3C0` derives this normal only when initial geometry is unavailable.
    ///
    /// Both normalizations retain extended values until the final XYZ stores.
    /// Zero and vertical-only travel preserve the native degenerate result.
    #[must_use]
    pub fn unavailable_geometry_normal(self) -> Vec3 {
        let mut direction = self.delta;
        let square =
            (direction.z * direction.z + direction.y * direction.y) + direction.x * direction.x;
        let threshold = f64::from(f32::from_bits(0x3480_0000));
        if square > threshold {
            direction *= 1.0 / square.sqrt();
        }
        let mut side_x = direction.y;
        let mut side_y = -direction.x;
        let square = side_y * side_y + side_x * side_x;
        if square > threshold {
            let inverse = 1.0 / square.sqrt();
            side_x *= inverse;
            side_y *= inverse;
        }
        Vec3::new(
            (side_y * direction.z) as f32,
            -(direction.z * side_x) as f32,
            (side_x * direction.y - side_y * direction.x) as f32,
        )
    }
}
