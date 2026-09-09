//! Fixed travel resolved by 762E00 before its collision-response substeps.

use glam::Vec3;
use thiserror::Error;

/// 75EE00 chooses whether the initial displacement retains its vertical axis.
#[derive(Clone, Copy, Debug)]
pub enum MovementTravelAxes {
    /// Ordinary ground/fall movement measures horizontal travel only.
    Horizontal,
    /// Swimming, flight, and admitted spatial paths measure all three axes.
    Spatial,
}

/// Direction and float-stored speed retained across one collision interval.
#[derive(Clone, Copy, Debug)]
pub struct MovementIntervalDrive {
    direction: Vec3,
    speed: f32,
}

/// Invalid displacement, empty duration, or an unrepresentable speed.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
#[error("movement interval travel is invalid")]
pub struct MovementIntervalDriveError;

impl MovementIntervalDrive {
    /// Resolves 762E76..762F05 using the already float-stored displacement.
    /// The square root remains extended through speed division and normalization.
    ///
    /// # Errors
    /// Rejects nonfinite travel, zero duration, or a nonfinite resulting speed.
    pub fn new(
        mut delta: Vec3,
        duration_ms: u32,
        axes: MovementTravelAxes,
    ) -> Result<Self, MovementIntervalDriveError> {
        if !delta.is_finite() || duration_ms == 0 {
            return Err(MovementIntervalDriveError);
        }
        if matches!(axes, MovementTravelAxes::Horizontal) {
            delta.z = 0.0;
        }
        let delta = delta.as_dvec3();
        let distance = ((delta.x * delta.x + delta.y * delta.y) + delta.z * delta.z).sqrt();
        let speed = (distance / (f64::from(duration_ms) * f64::from(0.001_f32))) as f32;
        if !speed.is_finite() {
            return Err(MovementIntervalDriveError);
        }
        let direction = if distance.abs() >= f64::from(f32::from_bits(0x3580_0000)) {
            (delta * (1.0 / distance)).as_vec3()
        } else {
            Vec3::ZERO
        };
        Ok(Self { direction, speed })
    }

    /// Returns the normalized float direction, or zero below the native threshold.
    #[must_use]
    pub const fn direction(self) -> Vec3 {
        self.direction
    }

    /// Returns the interval's retained speed in world units per second.
    #[must_use]
    pub const fn speed(self) -> f32 {
        self.speed
    }

    /// 762F30..762F6F recomputes distance from the remaining clock and fixed speed.
    #[must_use]
    pub fn distance(self, remaining_ms: u32) -> f32 {
        (f64::from(remaining_ms) * f64::from(0.001_f32) * f64::from(self.speed)) as f32
    }
}
