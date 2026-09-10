//! Passenger transition timing and world-model interpolation from build 12340.

use glam::Vec3;

const EPSILON: f64 = 0.000_1_f32 as f64;
const MILLISECONDS: f64 = 0.001_f32 as f64;

/// VehiclePassenger_C +14; delays and airborne travel have distinct entry work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum VehiclePassengerPhase {
    /// No unit vehicle owns the passenger model.
    #[default]
    Detached = 0,
    /// Hold the old pose before entering.
    EnterDelay = 1,
    /// Interpolate and arc toward the current seat anchor.
    Entering = 2,
    /// Follow the animated vehicle seat.
    Seated = 3,
    /// Hold the old seat before exiting.
    ExitDelay = 4,
    /// Interpolate and arc toward the unit's movement position.
    Exiting = 5,
}

impl VehiclePassengerPhase {
    /// Resolves 749AA0's next phase after movement has admitted a parent change.
    #[must_use]
    pub fn admit(
        unit_parent: bool,
        animated: bool,
        flags: u32,
        special_exit: bool,
        parameters: [f32; 7],
    ) -> Self {
        let [delay, speed, ..] = parameters;
        if !animated || f64::from(speed) <= EPSILON {
            return if unit_parent {
                Self::Seated
            } else {
                Self::Detached
            };
        }
        let enabled = flags
            & if unit_parent {
                1
            } else if special_exit {
                8
            } else {
                0x8000
            }
            != 0;
        match (unit_parent, enabled, f64::from(delay) > EPSILON) {
            (true, true, true) => Self::EnterDelay,
            (true, true, false) => Self::Entering,
            (true, false, _) => Self::Seated,
            (false, true, true) => Self::ExitDelay,
            (false, true, false) => Self::Exiting,
            (false, false, _) => Self::Detached,
        }
    }

    /// States whose world position follows the parabolic interpolation path.
    #[must_use]
    pub const fn airborne(self) -> bool {
        matches!(self, Self::Entering | Self::Exiting)
    }
}

/// World snapshots consumed when 74A200 initializes one delay or travel phase.
#[derive(Clone, Copy, Debug)]
pub struct VehicleTransitionInput {
    /// Phase selected by passenger admission or delay completion.
    pub phase: VehiclePassengerPhase,
    /// A live unit parent enables projected relative entry speed.
    pub has_parent: bool,
    /// Pre-delay, speed, gravity, min/max duration, and min/max arc height.
    pub parameters: [f32; 7],
    /// Current model origin saved by phase entry.
    pub origin: Vec3,
    /// Current entry seat anchor, or exit movement position.
    pub target: Vec3,
    /// The unit's current world movement position.
    pub unit_position: Vec3,
    /// Parent movement owner's retained direction times speed (6FED70).
    pub parent_velocity: Vec3,
    /// Current unit facing, before wrapping around the saved model facing.
    pub yaw: f32,
    /// Saved model facing from the preceding phase.
    pub previous_yaw: f32,
    /// Wrapping scene clock at phase entry.
    pub start_ms: u32,
}

/// Retained timing and yaw lanes for one passenger delay or travel phase.
#[derive(Clone, Copy, Debug)]
pub struct VehiclePassengerTransition {
    phase: VehiclePassengerPhase,
    start_ms: u32,
    end_ms: u32,
    gravity: f32,
    arc_scale: f32,
    start_yaw: f32,
    yaw: f32,
    target_yaw: f32,
}

/// World-model interpolation result; scale remains owned by the unit model.
#[derive(Clone, Copy, Debug)]
pub struct VehicleTransitionPose {
    /// Interpolated model origin including vertical arc displacement.
    pub position: Vec3,
    /// Wrapped, interpolated world facing.
    pub yaw: f32,
    /// Clamped and seat-eased travel fraction.
    pub fraction: f32,
}

impl VehiclePassengerTransition {
    /// Initializes native delay/duration and gravity/arc lanes (74A200).
    #[must_use]
    pub fn new(input: VehicleTransitionInput) -> Self {
        let [
            delay,
            speed,
            mut gravity,
            minimum,
            maximum,
            arc_min,
            arc_max,
        ] = input.parameters;
        let target_yaw = wrapped_target(input.yaw, input.previous_yaw);
        let mut arc_scale = 0.;
        let duration = if matches!(
            input.phase,
            VehiclePassengerPhase::EnterDelay | VehiclePassengerPhase::ExitDelay
        ) {
            gravity = 0.;
            f64::from(delay.min(10.))
        } else {
            let max_duration = if f64::from(maximum) < EPSILON {
                1.5
            } else {
                maximum.min(10.)
            };
            let duration = if input.phase == VehiclePassengerPhase::Entering && input.has_parent {
                let delta = input.target - input.origin;
                let [x, y, z] = delta.to_array().map(f64::from);
                let length = ((y * y + z * z) + x * x).sqrt() as f32;
                if f64::from(length) <= EPSILON {
                    0.
                } else {
                    let direction = delta
                        .to_array()
                        .map(|value| (f64::from(value) / f64::from(length)) as f32);
                    let [vx, vy, vz] = input.parent_velocity.to_array().map(f64::from);
                    let [dx, dy, dz] = direction.map(f64::from);
                    let relative_speed = f64::from(speed) - ((vy * dy + vz * dz) + vx * dx);
                    if relative_speed > EPSILON {
                        clamp_duration(
                            (f64::from(length) / relative_speed) as f32,
                            minimum,
                            max_duration,
                        )
                    } else {
                        max_duration
                    }
                }
            } else {
                let [x, y, z] = std::array::from_fn::<_, 3, _>(|lane| {
                    f64::from(input.unit_position[lane]) - f64::from(input.origin[lane])
                });
                clamp_duration(
                    (((z * z + y * y) + x * x).sqrt() / f64::from(speed)) as f32,
                    minimum,
                    max_duration,
                )
            };
            if f64::from(gravity.abs()) < EPSILON && (arc_min > 0. || arc_max < 0.) {
                gravity = 0.1;
            }
            arc_scale = 1.;
            if f64::from(duration) > EPSILON {
                let height = f64::from(gravity) * f64::from(duration) * f64::from(duration) * 0.125;
                if height < f64::from(arc_min) {
                    arc_scale = (f64::from(arc_min) / height) as f32;
                } else if height > f64::from(arc_max) {
                    arc_scale = (f64::from(arc_max) / height) as f32;
                }
            }
            f64::from(duration)
        };
        Self {
            phase: input.phase,
            start_ms: input.start_ms,
            end_ms: input
                .start_ms
                .wrapping_sub((-1000. * duration).trunc() as i64 as u32),
            gravity,
            arc_scale,
            start_yaw: input.previous_yaw,
            yaw: input.previous_yaw,
            target_yaw,
        }
    }

    /// End timestamp after native truncation toward zero.
    #[must_use]
    pub const fn end_ms(self) -> u32 {
        self.end_ms
    }
    /// Gravity retained by the phase, including the zero-gravity fallback.
    #[must_use]
    pub const fn gravity(self) -> f32 {
        self.gravity
    }
    /// Multiplier that satisfies the authored minimum/maximum arc height.
    #[must_use]
    pub const fn arc_scale(self) -> f32 {
        self.arc_scale
    }
    /// Current unit facing wrapped around the previously published model facing.
    #[must_use]
    pub const fn target_yaw(self) -> f32 {
        self.target_yaw
    }
    /// Signed wrapping deadline test used by 74AF70.
    #[must_use]
    pub fn finished(self, now_ms: u32) -> bool {
        now_ms.wrapping_sub(self.end_ms) as i32 >= 0
    }

    /// Samples 747D70 easing, 747A30 yaw, and 74A7F0's airborne model origin.
    #[must_use]
    pub fn sample(
        &mut self,
        now_ms: u32,
        flags: u32,
        origin: Vec3,
        target: Vec3,
        yaw: f32,
    ) -> VehicleTransitionPose {
        let duration = f64::from(self.end_ms.wrapping_sub(self.start_ms) as i32) * MILLISECONDS;
        let mut fraction = if duration < EPSILON {
            1.
        } else {
            ((f64::from(now_ms.wrapping_sub(self.start_ms) as i32) * MILLISECONDS / duration)
                as f32)
                .clamp(0., 1.)
        };
        if self.phase.airborne() {
            let (ease_in, ease_out) = if self.phase == VehiclePassengerPhase::Entering {
                (0x20, 0x40)
            } else {
                (0x80, 0x100)
            };
            let f = f64::from(fraction);
            fraction = match (flags & ease_in != 0, flags & ease_out != 0) {
                (true, true) => (0.5 - (f * f64::from(std::f32::consts::PI)).cos() * 0.5) as f32,
                (true, false) => (f * f) as f32,
                (false, true) => (1. - (1. - f) * (1. - f)) as f32,
                (false, false) => fraction,
            };
            self.target_yaw = wrapped_target(yaw, self.yaw);
            self.yaw = (f64::from(self.target_yaw) * f64::from(fraction)
                + (1. - f64::from(fraction)) * f64::from(self.start_yaw))
                as f32;
        }
        let position = if self.phase.airborne() {
            let t = f64::from(fraction) * duration;
            let half_g_t = f64::from(self.gravity) * t * 0.5;
            let arc = if f64::from(self.gravity.abs()) <= EPSILON {
                0.
            } else {
                ((half_g_t * duration - half_g_t * t) * f64::from(self.arc_scale)) as f32
            };
            Vec3::from_array(std::array::from_fn(|lane| {
                let position = f64::from(origin[lane]) * (1. - f64::from(fraction))
                    + f64::from(target[lane]) * f64::from(fraction);
                (position + if lane == 2 { f64::from(arc) } else { 0. }) as f32
            }))
        } else {
            origin
        };
        VehicleTransitionPose {
            position,
            yaw: self.yaw,
            fraction,
        }
    }
}

fn clamp_duration(value: f32, minimum: f32, maximum: f32) -> f32 {
    value.max(minimum).min(maximum)
}

fn wrapped_target(mut target: f32, previous: f32) -> f32 {
    let pi = f64::from(std::f32::consts::PI);
    let tau = std::f32::consts::TAU;
    while f64::from(target) + pi < f64::from(previous) {
        target += tau;
    }
    while f64::from(previous) < f64::from(target) - pi {
        target -= tau;
    }
    target
}
