//! Analytic falling distance and contact time from stock `Movement_C.cpp`.

use thiserror::Error;

// Build-12340 scalar images: 0x00A32F74, 0x00AA33AC, 0x00A37F8C,
// 0x00AA33D8, 0x00AA33D4, and 0x009EA27C. Keeping each native constant
// avoids replacing independently rounded reciprocal values with division.
const GRAVITY: f32 = f32::from_bits(0x419a_542f);
const HALF_GRAVITY: f32 = f32::from_bits(0x411a_542f);
const INVERSE_GRAVITY: f32 = f32::from_bits(0x3d54_536a);
const DOUBLE_GRAVITY: f32 = f32::from_bits(0x421a_542f);
const DOUBLE_INVERSE_GRAVITY: f32 = f32::from_bits(0x3dd4_536a);
const ZERO_LAUNCH_TOLERANCE: f32 = f32::from_bits(0x3480_0000);
const SECONDS_PER_MILLISECOND: f32 = f32::from_bits(0x3a83_126f);

/// Terminal-speed choice already resolved by the movement effect owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementFallMode {
    /// Native ordinary terminal speed at `0x00B2D9E8`.
    Normal,
    /// Native slow-fall terminal speed at `0x00B2D9EC`.
    Slow,
}

impl MovementFallMode {
    /// Returns the native terminal speed in world units per second.
    const fn terminal_speed(self) -> f32 {
        match self {
            Self::Normal => f32::from_bits(0x4270_978e),
            Self::Slow => 7.0,
        }
    }
}

/// Chooses a contact-time root when a jump crosses one height twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementFallCrossing {
    /// Uses the earlier root, clamped to zero for a nonzero launch. The native
    /// stationary-launch branch uses its sole root regardless of this request.
    Ascending,
    /// Uses the later descending root, including terminal-speed travel.
    Descending,
}

/// Invalid scalar input at the analytic trajectory boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovementFallError {
    /// Initial downward speed is NaN or infinite.
    #[error("movement fall launch speed is not finite")]
    NonFiniteLaunchSpeed,
    /// Elapsed time is negative, NaN, or infinite.
    #[error("movement fall elapsed time is invalid")]
    InvalidElapsedTime,
    /// Downward displacement is NaN or infinite.
    #[error("movement fall distance is not finite")]
    NonFiniteDistance,
    /// An otherwise finite query cannot be represented by a native float.
    #[error("movement fall result is not finite")]
    NonFiniteResult,
}

/// A native ballistic trajectory with downward-positive launch and displacement.
///
/// Negative launch speed rises before falling. This value owns the analytic
/// curve only; collision and event owners retain the origin, clock, flags, and
/// horizontal launch state. Changing the mode requires a new resolved curve.
#[derive(Clone, Copy, Debug)]
pub struct MovementFallTrajectory {
    mode: MovementFallMode,
    initial_downward_speed: f32,
}

impl MovementFallTrajectory {
    /// Admits the resolved fall mode and signed downward launch speed.
    ///
    /// The native curve caps positive launch speed at the selected terminal
    /// speed. Negative upward launch speed retains its full magnitude.
    ///
    /// # Errors
    /// Returns [`MovementFallError::NonFiniteLaunchSpeed`] for NaN or infinity.
    pub fn new(
        mode: MovementFallMode,
        initial_downward_speed: f32,
    ) -> Result<Self, MovementFallError> {
        if !initial_downward_speed.is_finite() {
            return Err(MovementFallError::NonFiniteLaunchSpeed);
        }
        Ok(Self {
            mode,
            initial_downward_speed,
        })
    }

    /// Samples downward distance from the launch origin (`0x00986F00`).
    ///
    /// This is an absolute curve sample, not a per-frame gravity accumulation.
    /// Once terminal speed is reached, the remaining interval is linear.
    ///
    /// # Errors
    /// Returns [`MovementFallError`] for invalid time or a non-finite result.
    pub fn distance_at_seconds(self, elapsed_seconds: f32) -> Result<f32, MovementFallError> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return Err(MovementFallError::InvalidElapsedTime);
        }
        let terminal = f64::from(self.mode.terminal_speed());
        let launch = f64::from(self.initial_downward_speed.min(self.mode.terminal_speed()));
        let elapsed = f64::from(elapsed_seconds);
        let distance = if elapsed * f64::from(GRAVITY) + launch >= terminal {
            let terminal_time = (terminal - launch) * f64::from(INVERSE_GRAVITY);
            (f64::from(HALF_GRAVITY) * terminal_time + launch) * terminal_time
                + (elapsed - terminal_time) * terminal
        } else {
            (elapsed * f64::from(HALF_GRAVITY) + launch) * elapsed
        };
        finite_result(distance)
    }

    /// Samples a native unsigned millisecond clock (`0x00987050`).
    ///
    /// Stock retains the exact integer in x87 through multiplication by the
    /// native seconds-per-millisecond constant, then stores seconds as a float.
    /// Converting milliseconds to f32 first would lose ticks above 2^24.
    ///
    /// # Errors
    /// Returns [`MovementFallError::NonFiniteResult`] if the curve result exceeds
    /// the native float range.
    pub fn distance_at_millis(self, elapsed_ms: u32) -> Result<f32, MovementFallError> {
        let seconds = (f64::from(elapsed_ms) * f64::from(SECONDS_PER_MILLISECOND)) as f32;
        self.distance_at_seconds(seconds)
    }

    /// Finds the native time for a downward distance (`0x00988220`/`0x00988280`).
    ///
    /// Negative distance denotes a point above launch. Stock clamps a negative
    /// discriminant to zero, so a height above the apex returns the apex time;
    /// this query deliberately preserves that behavior instead of reporting
    /// geometric reachability. A stationary launch maps negative distance to zero.
    /// For an initially downward launch, the descending root for a point above
    /// the launch height can be negative; stock retains that signed result.
    ///
    /// # Errors
    /// Returns [`MovementFallError`] for non-finite distance or result.
    pub fn seconds_at_distance(
        self,
        downward_distance: f32,
        crossing: MovementFallCrossing,
    ) -> Result<f32, MovementFallError> {
        finite_result(self.seconds_at_distance_extended(downward_distance, crossing)?)
    }

    /// Collision callers retain the inverse root in x87 until after subtracting
    /// the interval's start time. Preserve that boundary before narrowing.
    pub(crate) fn seconds_at_distance_extended(
        self,
        downward_distance: f32,
        crossing: MovementFallCrossing,
    ) -> Result<f64, MovementFallError> {
        if !downward_distance.is_finite() {
            return Err(MovementFallError::NonFiniteDistance);
        }
        let terminal = f64::from(self.mode.terminal_speed());
        let launch = f64::from(self.initial_downward_speed.min(self.mode.terminal_speed()));
        let distance = f64::from(downward_distance);
        let inverse_gravity = f64::from(INVERSE_GRAVITY);
        if launch.abs() < f64::from(ZERO_LAUNCH_TOLERANCE) {
            let terminal_distance = inverse_gravity * terminal * terminal * 0.5;
            let elapsed = if distance >= terminal_distance {
                inverse_gravity * terminal + (distance - terminal_distance) / terminal
            } else if distance <= 0.0 {
                0.0
            } else {
                (distance * f64::from(DOUBLE_INVERSE_GRAVITY)).sqrt()
            };
            return Ok(elapsed);
        }
        let discriminant = f64::from(DOUBLE_GRAVITY) * distance + launch * launch;
        let root = discriminant.max(0.0).sqrt();
        let ascending = (-launch - root) * inverse_gravity;
        if crossing == MovementFallCrossing::Ascending {
            return Ok(ascending.max(0.0));
        }
        let descending = (root - launch) * inverse_gravity;
        let terminal_time = (terminal - launch) * inverse_gravity;
        let elapsed = if descending > terminal_time {
            let terminal_distance =
                (f64::from(HALF_GRAVITY) * terminal_time + launch) * terminal_time;
            terminal_time + (distance - terminal_distance) / terminal
        } else {
            descending
        };
        Ok(elapsed)
    }

    /// `0x00986E80` uses the signed, uncapped launch field for an active fall.
    pub(crate) fn apex_seconds(self) -> f64 {
        -f64::from(self.initial_downward_speed) * f64::from(INVERSE_GRAVITY)
    }
}

/// Narrows the x87-style intermediate at the native float result boundary.
fn finite_result(value: f64) -> Result<f32, MovementFallError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MovementFallError::NonFiniteResult)
    }
}
