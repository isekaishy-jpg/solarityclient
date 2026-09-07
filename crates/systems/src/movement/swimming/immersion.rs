//! Unit_C's registered-liquid decisions (`0x00730D10`).

use thiserror::Error;

/// Deferred event produced by the unit update; applying it owns packet emission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementSwimTransition {
    /// Event 0x15 enters swimming through 989660.
    Enter,
    /// Event 0x16 leaves swimming through 98BFF0.
    Leave,
}

/// Inputs retained by the unit and its embedded movement owner.
#[derive(Clone, Copy, Debug)]
pub struct MovementSwimImmersion {
    /// Primary movement flags, including swimming, ascending, and flying.
    pub flags: u32,
    /// Secondary movement flags; bit 4 bypasses the whole unit-water update.
    pub secondary: u16,
    /// UNIT_FIELD_FLAGS consumed by 4F53D0's swim-capability predicate.
    pub unit_flags: u32,
    /// Unit virtual +0x40's attached movement parent GUID.
    pub parent_guid: u64,
    /// 4D43C0's local-control admission for surface-jump requests.
    pub locally_controlled: bool,
    /// Unit model collision height in world units.
    pub height: f32,
    /// Registered surface minus unit world Z; absence is dry space.
    pub liquid_depth: Option<f32>,
    /// Unit +0x784's previous splash-depth sample.
    pub previous_depth: f32,
    /// Elapsed time since the current fall launch, in milliseconds.
    pub fall_time_ms: u32,
    /// Fall launch speed, positive downwards.
    pub initial_downward_speed: f32,
}

/// Decisions before deferred movement events change the owner's flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementSwimImmersionUpdate {
    /// Entry or exit notification to enqueue at the current unit-update time.
    pub transition: Option<MovementSwimTransition>,
    /// 72EB80's surface-jump request; the jump command owns further admission.
    pub attempt_surface_jump: bool,
    /// Crossing 40% height invokes splash audio (746720) and ripple kind 3 (71CBA0).
    pub splash: bool,
    /// Unit +0xA30 bit 0x200000, independent of deferred swim flags.
    pub is_swimming: bool,
    /// Next value of the retained splash-depth lane.
    pub previous_depth: f32,
}

/// An immersion sample cannot represent native finite model and movement inputs.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("invalid swimming immersion input")]
pub struct MovementSwimImmersionError;

impl MovementSwimImmersion {
    /// Evaluates the original entry/exit and splash thresholds without executing
    /// their downstream commands. `None` preserves every lane for no-collision.
    ///
    /// # Errors
    /// Rejects non-finite inputs and a nonpositive model height.
    pub fn evaluate(
        self,
    ) -> Result<Option<MovementSwimImmersionUpdate>, MovementSwimImmersionError> {
        if !self.height.is_finite()
            || self.height <= 0.0
            || self.liquid_depth.is_some_and(|depth| !depth.is_finite())
            || !self.previous_depth.is_finite()
            || !self.initial_downward_speed.is_finite()
        {
            return Err(MovementSwimImmersionError);
        }
        if self.secondary & 4 != 0 {
            return Ok(None);
        }
        let swimming = self.flags & 0x20_0000 != 0;
        let eligible = self.unit_flags & 0x4000 == 0
            && self.unit_flags & (8 | 0x10 | 0x800 | 0x8000) != 0
            && self.parent_guid == 0;
        let depth = self.liquid_depth.unwrap_or(0.0);
        // 730DAB retains the extended product for entry/surface-jump decisions,
        // but IsSwimming reads its float store. Exit subtracts before that store.
        let threshold = f64::from(self.height) * 0.75;
        let exit_threshold = (threshold - f64::from(f32::from_bits(0x3ce3_8e39))) as f32;
        let rising = self.flags & 0x1000 != 0
            && self.initial_downward_speed != 0.0
            && f64::from(self.fall_time_ms) * f64::from(0.001_f32)
                < f64::from(self.initial_downward_speed) * f64::from(f32::from_bits(0xbd54_536a));
        let transition = if !swimming && f64::from(depth) > threshold && eligible && !rising {
            Some(MovementSwimTransition::Enter)
        } else if swimming
            && self.flags & 0x200_0000 == 0
            && (self.liquid_depth.is_none() || depth < exit_threshold || !eligible)
        {
            Some(MovementSwimTransition::Leave)
        } else {
            None
        };
        let splash_threshold = f64::from(self.height) * f64::from(0.4_f32);
        Ok(Some(MovementSwimImmersionUpdate {
            transition,
            attempt_surface_jump: swimming
                && self.flags & 0x40_0000 != 0
                && f64::from(depth) - threshold <= f64::from(1.0_f32 / 3.0)
                && self.locally_controlled,
            splash: !swimming
                && ((f64::from(depth) > splash_threshold)
                    != (f64::from(self.previous_depth) > splash_threshold)),
            is_swimming: swimming || (depth > threshold as f32 && eligible),
            previous_depth: if swimming { self.previous_depth } else { depth },
        }))
    }
}
