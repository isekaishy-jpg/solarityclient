//! Retained nonspline unit body yaw and semantic spine/head rotations.

#[cfg(test)]
#[path = "../../tests/support/body_orientation.rs"]
mod tests;

const PI: f64 = f32::from_bits(0x4049_0fdb) as f64;
const TAU: f64 = f32::from_bits(0x40c9_0fdb) as f64;
const HALF_PI: f64 = f32::from_bits(0x3fc9_0fdb) as f64;
const QUARTER_PI: f64 = f32::from_bits(0x3f49_0fdb) as f64;
const SMALL_ANGLE: f64 = f32::from_bits(0x3727_c5ac) as f64;

/// Scene inputs to the ordinary `Unit_C` orientation controller at `73DAB0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitBodyOrientationInput {
    /// Authoritative unit facing in radians.
    pub facing: f32,
    /// Living movement flags, including strafe, turn and aquatic modes.
    pub movement_flags: u32,
    /// Authored turn speed from the living movement speed vector.
    pub turn_rate: f32,
    /// Time since the facing owner last reset its catch-up clock.
    pub facing_elapsed_ms: u32,
    /// Current scene's elapsed seconds.
    pub frame_seconds: f32,
    /// A keyboard or admitted mouse turn currently owns facing.
    pub direct_facing: bool,
    /// The controlled-unit policy assigns all available twist to the spine.
    pub full_spine_turn: bool,
    /// The primary model has semantic spine key bone four.
    pub has_spine: bool,
    /// The primary model has semantic head key bone six.
    pub has_head: bool,
    /// A separate mount model suppresses the rider's spine twist.
    pub mounted: bool,
    /// The unit has positive health and no vehicle override for body yaw.
    pub body_yaw_allowed: bool,
}

/// One independently retained unit body controller, outside GPU residency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitBodyOrientation {
    yaw: f32,
    velocity: f32,
    smoothing: f32,
    spine: Option<f32>,
    head: Option<f32>,
}

/// Model yaw, local bone overrides, and the native procedural turn flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitBodyOrientationSample {
    /// The model's smoothed body yaw, distinct from gameplay facing.
    pub yaw: f32,
    /// Additional spine rotation around local Z, when its override is enabled.
    pub spine: Option<f32>,
    /// Additional head rotation around local Z, when its override is enabled.
    pub head: Option<f32>,
    /// Native `0x800` / `0x1000` flags for left / right idle catch-up.
    pub procedural_turn: u32,
}

impl UnitBodyOrientation {
    /// Starts a newly resident unit facing its authoritative orientation.
    #[must_use]
    pub const fn new(facing: f32) -> Self {
        Self {
            yaw: facing,
            velocity: 0.0,
            smoothing: 0.0,
            spine: None,
            head: None,
        }
    }

    /// Advances the original body-yaw and spine/head policy once per scene.
    pub fn advance(&mut self, input: UnitBodyOrientationInput) -> UnitBodyOrientationSample {
        let flags = input.movement_flags;
        let facing = f64::from(input.facing);
        let dt = f64::from(input.frame_seconds);
        if !input.body_yaw_allowed || flags & 0x2200000 != 0 || !(input.has_spine || input.has_head)
        {
            self.yaw = input.facing;
            self.velocity = 0.0;
            self.smoothing = 0.0;
        } else if flags & 0xc != 0 {
            let mut angle = if flags & 3 != 0 { QUARTER_PI } else { HALF_PI };
            if matches!(flags & 6, 0 | 6) {
                angle = -angle;
            }
            self.smoothing = 1.0;
            let difference = wrap_signed((facing - f64::from(self.yaw) + angle) as f32 as f64);
            let target = wrap_signed((difference + f64::from(self.yaw)) as f32 as f64) as f32;
            self.damp_yaw(input.facing, target, input.frame_seconds);
        } else if flags & 0x1003 != 0 {
            if self.smoothing > 0.0 {
                self.smoothing = (f64::from(self.smoothing) - dt * 2.5).min(1.0) as f32;
                self.damp_yaw(input.facing, input.facing, input.frame_seconds);
            } else {
                self.yaw = input.facing;
                self.velocity = 0.0;
            }
        } else if self.smoothing < 1.0 {
            self.smoothing = (f64::from(self.smoothing) + dt * 2.5) as f32;
        }

        let difference = wrap_signed(facing - f64::from(self.yaw));
        let mut correction = 0.0_f32;
        if difference.abs() < f64::from(f32::from_bits(0x3a83_126f)) {
            self.spine = None;
            self.head = None;
        } else {
            if difference.abs() > HALF_PI {
                correction = ((difference.abs() - HALF_PI) as f32).copysign(difference as f32);
            }
            let mut yaw_correction = f64::from(correction);
            if flags & 0xc == 0 && !input.direct_facing {
                let catch_up = (f64::from(input.facing_elapsed_ms)
                    * f64::from(f32::from_bits(0x3a83_126f))
                    * f64::from(input.turn_rate)
                    * 8.0)
                    .min(f64::from(difference.abs() as f32)) as f32;
                yaw_correction += f64::from(catch_up.copysign(difference as f32));
                correction = yaw_correction as f32;
            }
            // Native retains the wider sum through wrapping, then stores yaw.
            let yaw = wrap_signed(f64::from(self.yaw) + yaw_correction);
            self.yaw = yaw as f32;
            let mut remaining = wrap_signed(facing - yaw).abs();
            if remaining < SMALL_ANGLE {
                self.spine = None;
                self.head = None;
            } else {
                let stored_remaining = remaining as f32;
                if !input.mounted && input.has_spine {
                    let spine = (remaining * if input.full_spine_turn { 1.0 } else { 0.5 })
                        .min(QUARTER_PI) as f32;
                    self.spine = Some(spine.copysign(difference as f32));
                    remaining = f64::from(stored_remaining) - f64::from(spine);
                }
                if input.has_head {
                    self.head =
                        Some((remaining.min(QUARTER_PI) as f32).copysign(difference as f32));
                }
            }
        }
        let procedural_turn = if flags & 0x2e0100f != 0 {
            0
        } else if f64::from(correction) > SMALL_ANGLE {
            0x800
        } else if f64::from(correction) < -SMALL_ANGLE {
            0x1000
        } else {
            0
        };
        UnitBodyOrientationSample {
            yaw: self.yaw,
            spine: self.spine,
            head: self.head,
            procedural_turn,
        }
    }

    /// `719660`: circular target alignment followed by the native damped step.
    fn damp_yaw(&mut self, facing: f32, target: f32, frame_seconds: f32) {
        let mut facing = f64::from(facing);
        let yaw = f64::from(self.yaw);
        if yaw > facing + PI {
            facing += TAU;
        } else if yaw < facing - PI {
            facing -= TAU;
        }
        let limit = f64::from(f32::from_bits(0x3fc9_0590));
        if facing > yaw + limit {
            self.yaw = (facing - limit) as f32;
        } else if facing < yaw - limit {
            self.yaw = (facing + limit) as f32;
        }
        let yaw = wrap_signed(f64::from(self.yaw));
        self.yaw = yaw as f32;
        let mut target = f64::from(target);
        if yaw > target + PI {
            target += TAU;
        } else if yaw < target - PI {
            target -= TAU;
        }
        let dt = f64::from(frame_seconds);
        let omega = 20.0;
        let x = dt * omega;
        let decay = 1.0
            / (x * x * f64::from(f32::from_bits(0x3ef5_c28f))
                + x * x * x * f64::from(f32::from_bits(0x3e70_a3d7))
                + x
                + 1.0);
        let difference = f64::from(self.yaw) - target;
        let change = (difference * omega + f64::from(self.velocity)) * dt;
        self.yaw = ((difference + change) * decay + target) as f32;
        self.velocity = ((f64::from(self.velocity) - change * omega) * decay) as f32;
        self.yaw = (f64::from(self.yaw) * f64::from(self.smoothing)
            + (1.0 - f64::from(self.smoothing)) * target) as f32;
    }
}

fn wrap_signed(value: f64) -> f64 {
    if value > PI {
        (value + PI) % TAU - PI
    } else if value < -PI {
        (value - PI) % TAU + PI
    } else {
        value
    }
}
