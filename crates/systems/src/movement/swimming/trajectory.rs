//! Anchored 3D input integration from 987EF0 and 987B50.

use glam::{Vec2, Vec3};
use solarity_ecs::WorldMovementSpeeds;
use thiserror::Error;

use super::super::clock::seconds_from_millis;
use super::super::ground_trajectory::MovementYawTrajectory;

const DIAGONAL: f32 = f32::from_bits(0x3f35_04f3);

/// Analytic input basis retained until native movement reanchors.
#[derive(Clone, Copy, Debug)]
pub struct MovementSwimTrajectory {
    flags: u32,
    direction: Vec3,
    horizontal: Vec2,
    pitch_basis: Vec2,
    speed: f32,
    yaw: MovementYawTrajectory,
    yaw_rate: f64,
    pitch: f32,
    pitch_rate: f64,
}

/// Displacement, facing, and pitch relative to the same movement anchor.
#[derive(Clone, Copy, Debug)]
pub struct MovementSwimSample {
    /// Three-dimensional displacement from the retained position anchor.
    pub displacement: Vec3,
    /// Facing wrapped with the stock full-turn constant.
    pub orientation: f32,
    /// Pitch retains native signed remainder until command-side admission.
    pub pitch: f32,
}

/// Invalid scalar or unsupported mode at the swimming trajectory boundary.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("swim trajectory requires finite swimming state and nonnegative speeds")]
pub struct MovementSwimTrajectoryError;

impl MovementSwimTrajectory {
    /// Changes the retained bases and yaw without restarting analytic time.
    ///
    /// # Errors
    /// Rejects non-finite transformed bases or yaw.
    pub fn rebased(
        self,
        change: super::super::MovementTransportChange,
    ) -> Result<Self, MovementSwimTrajectoryError> {
        let direction = change.direction(self.direction);
        let horizontal = change.direction(self.horizontal.extend(0.0)).truncate();
        if !direction.is_finite() || !horizontal.is_finite() {
            return Err(MovementSwimTrajectoryError);
        }
        Ok(Self {
            direction,
            horizontal,
            yaw: self
                .yaw
                .rebased(change)
                .map_err(|_| MovementSwimTrajectoryError)?,
            ..self
        })
    }
    /// Resolves native 987570 speed and 987EF0 movement bases.
    ///
    /// # Errors
    /// Rejects absent swimming, falling/flight/spline modes, or invalid scalars.
    pub fn new(
        flags: u32,
        secondary: u16,
        orientation: f32,
        pitch: f32,
        speeds: WorldMovementSpeeds,
    ) -> Result<Self, MovementSwimTrajectoryError> {
        if flags & 0x200000 == 0
            || flags & 0x4a00_1000 != 0
            || !orientation.is_finite()
            || !pitch.is_finite()
            || speeds
                .values()
                .iter()
                .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(MovementSwimTrajectoryError);
        }
        let (sin, cos) = f64::from(orientation).sin_cos();
        let forward = Vec2::new(cos as f32, sin as f32);
        let pitch_basis = if pitch.abs() >= f32::from_bits(0x3580_0000) {
            let (sin, cos) = f64::from(pitch).sin_cos();
            Vec2::new(cos as f32, sin as f32)
        } else {
            Vec2::X
        };
        let mut horizontal = forward;
        let mut direction = (forward * pitch_basis.x).extend(pitch_basis.y);
        if flags & 2 != 0 {
            horizontal = -horizontal;
            direction = -direction;
        }
        if flags & 0xc != 0 {
            let strafe = if flags & 4 != 0 {
                Vec2::new(-forward.y, forward.x)
            } else {
                Vec2::new(forward.y, -forward.x)
            };
            if flags & 3 != 0 {
                horizontal = (horizontal + strafe) * DIAGONAL;
                direction =
                    ((direction.truncate() + strafe) * DIAGONAL).extend(direction.z * DIAGONAL);
            } else {
                horizontal = strafe;
                direction = strafe.extend(0.);
            }
        }
        let speed = if flags & 0xc0000f == 0 {
            0.
        } else if flags & 2 != 0 {
            speeds.swim_back().min(speeds.swim())
        } else {
            speeds.swim()
        };
        let yaw =
            MovementYawTrajectory::new(flags, secondary & 8 != 0, orientation, speeds.turn_rate())
                .map_err(|_| MovementSwimTrajectoryError)?;
        Ok(Self {
            flags,
            direction,
            horizontal,
            pitch_basis,
            speed,
            yaw,
            yaw_rate: angular_rate(flags, 0x10, 0x20, secondary & 8 != 0, speeds.turn_rate()),
            pitch,
            pitch_rate: angular_rate(
                flags,
                0x40,
                0x80,
                secondary & 0x10 != 0,
                speeds.pitch_rate(),
            ),
        })
    }

    /// Returns the pitched and strafed native travel basis.
    #[must_use]
    pub const fn direction(self) -> Vec3 {
        self.direction
    }

    /// Returns native 987570's swim speed, including backward admission.
    #[must_use]
    pub const fn speed(self) -> f32 {
        self.speed
    }

    /// Samples native 987B50's selected analytic trajectory.
    #[must_use]
    pub fn sample(self, elapsed_ms: u32) -> MovementSwimSample {
        let time = f64::from(seconds_from_millis(elapsed_ms));
        let orientation = self.yaw.sample(elapsed_ms);
        let pitch = if self.flags & 0xc0 != 0 && elapsed_ms != 0 {
            ((self.pitch_rate * time + f64::from(self.pitch))
                % f64::from(f32::from_bits(0x40c9_0fdb))) as f32
        } else {
            self.pitch
        };
        let mut selector = 0;
        if self.flags & 0x30 != 0 {
            selector |= 2;
        }
        if self.flags & 0xc00000 != 0 {
            selector |= 16;
        } else if self.flags & 0xc0 != 0 {
            selector |= 8;
        }
        if self.flags & 3 != 0 {
            selector |= 1;
        }
        if self.flags & 0xc != 0 {
            selector |= 4;
        }
        let displacement = if elapsed_ms == 0 {
            Vec3::ZERO
        } else {
            match selector {
                1 | 4 | 5 | 12 => {
                    (self.direction.as_dvec3() * time * f64::from(self.speed)).as_vec3()
                }
                3 | 6 | 7 | 14 | 19 | 22 | 23 => self.turning(time),
                9 | 13 => self.pitching(time),
                11 | 15 => self.turning_and_pitching(time),
                16 | 18 => Vec3::new(0., 0., (time * self.vertical_speed()) as f32),
                17 | 20 | 21 => {
                    let scale = f64::from(DIAGONAL);
                    Vec3::new(
                        (f64::from(self.horizontal.x) * scale * time * f64::from(self.speed))
                            as f32,
                        (f64::from(self.horizontal.y) * scale * time * f64::from(self.speed))
                            as f32,
                        (time * self.vertical_speed() * scale) as f32,
                    )
                }
                _ => Vec3::ZERO,
            }
        };
        MovementSwimSample {
            displacement,
            orientation,
            pitch,
        }
    }

    fn vertical_speed(self) -> f64 {
        if self.flags & 0x400000 != 0 {
            f64::from(self.speed)
        } else {
            -f64::from(self.speed)
        }
    }

    /// 987A00 preserves separate horizontal and pitch bases during yaw.
    fn turning(self, time: f64) -> Vec3 {
        let (across, along) = arc(f64::from(self.speed), self.yaw_rate, time);
        let x = f64::from(self.horizontal.x);
        let y = f64::from(self.horizontal.y);
        let horizontal = glam::DVec2::new(x * across - y * along, x * along + y * across);
        if self.flags & 0xc00000 != 0 {
            (horizontal * f64::from(DIAGONAL))
                .extend(self.vertical_speed() * time * f64::from(DIAGONAL))
                .as_vec3()
        } else if self.flags & 3 != 0 {
            (horizontal * f64::from(self.pitch_basis.x))
                .extend(f64::from(self.pitch_basis.y) * f64::from(self.speed) * time)
                .as_vec3()
        } else {
            horizontal.extend(0.).as_vec3()
        }
    }

    /// 987950 integrates a pitch arc while retaining the horizontal input basis.
    fn pitching(self, time: f64) -> Vec3 {
        let (across, along) = arc(f64::from(self.speed), self.pitch_rate, time);
        let cosine = f64::from(self.pitch_basis.x);
        let sine = f64::from(self.pitch_basis.y);
        let horizontal = across * cosine - along * sine;
        Vec3::new(
            (horizontal * f64::from(self.horizontal.x)) as f32,
            (horizontal * f64::from(self.horizontal.y)) as f32,
            (across * sine + along * cosine) as f32,
        )
    }

    /// 987820 integrates independent yaw and pitch arcs at diagonal speed.
    fn turning_and_pitching(self, time: f64) -> Vec3 {
        let speed = f64::from(self.speed) * f64::from(DIAGONAL);
        let (across, along) = arc(speed, self.yaw_rate, time);
        let (up, inward) = arc(speed, self.pitch_rate, time);
        Vec3::new(
            (f64::from(self.horizontal.x) * across - f64::from(self.horizontal.y) * along) as f32,
            (f64::from(self.horizontal.x) * along + f64::from(self.horizontal.y) * across) as f32,
            (up * f64::from(self.pitch_basis.y) + inward * f64::from(self.pitch_basis.x)) as f32,
        )
    }
}

fn angular_rate(flags: u32, positive: u32, negative: u32, full_speed: bool, speed: f32) -> f64 {
    let rate = if flags & positive != 0 {
        f64::from(speed)
    } else if flags & negative != 0 {
        -f64::from(speed)
    } else {
        0.
    };
    if flags & 0xc0100f != 0 && !full_speed {
        rate * 0.75
    } else {
        rate
    }
}

/// The native functions spill radius and trigonometric inputs to single precision.
fn arc(speed: f64, rate: f64, time: f64) -> (f64, f64) {
    let radius = f64::from((speed / rate) as f32);
    let (sin, cos) = f64::from((rate * time) as f32).sin_cos();
    (
        f64::from(sin as f32) * radius,
        radius - f64::from(cos as f32) * radius,
    )
}
