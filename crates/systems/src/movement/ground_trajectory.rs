//! Anchored horizontal motion from `0x00987EF0` and `0x00987B50`.

use glam::{Vec2, Vec3};
use solarity_ecs::WorldMovementSpeeds;
use thiserror::Error;

use super::clock::seconds_from_millis;
use super::transport::MovementTransportChange;

/// Ground input basis and analytic yaw, retained until movement reanchors.
#[derive(Clone, Copy, Debug)]
pub struct MovementGroundTrajectory {
    basis_xy: Vec2,
    basis_z: f32,
    direction: Vec2,
    speed: f32,
    yaw: MovementYawTrajectory,
}

/// Anchored facing shared by grounded, airborne, and vertical movement.
#[derive(Clone, Copy, Debug)]
pub struct MovementYawTrajectory {
    orientation: f32,
    rate: f64,
    turning: bool,
}

/// Invalid facing or turn speed at the analytic yaw boundary.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("yaw trajectory requires finite facing and finite nonnegative turn speed")]
pub struct MovementYawTrajectoryError;

impl MovementYawTrajectory {
    /// Changes the retained facing anchor while preserving turn rate and clock ownership.
    ///
    /// # Errors
    /// Rejects a facing that overflows during frame conversion.
    pub fn rebased(
        self,
        change: MovementTransportChange,
    ) -> Result<Self, MovementYawTrajectoryError> {
        let orientation = change.orientation(self.orientation);
        if !orientation.is_finite() {
            return Err(MovementYawTrajectoryError);
        }
        Ok(Self {
            orientation,
            ..self
        })
    }

    /// Resolves `0x00987770`'s turn rate from local movement flags.
    /// Falling and ascent/descent count as movement even without translation.
    ///
    /// # Errors
    /// Rejects invalid facing or turn speed.
    pub fn new(
        flags: u32,
        full_speed_turn: bool,
        orientation: f32,
        turn_speed: f32,
    ) -> Result<Self, MovementYawTrajectoryError> {
        if !orientation.is_finite() || !turn_speed.is_finite() || turn_speed < 0.0 {
            return Err(MovementYawTrajectoryError);
        }
        let mut rate = if flags & 0x10 != 0 {
            f64::from(turn_speed)
        } else if flags & 0x20 != 0 {
            -f64::from(turn_speed)
        } else {
            0.0
        };
        if flags & 0x00c0_100f != 0 && !full_speed_turn {
            rate *= 0.75;
        }
        Ok(Self {
            orientation,
            rate,
            turning: flags & 0x30 != 0,
        })
    }

    /// Samples facing using the native unsigned clock and full-turn constant.
    #[must_use]
    pub fn sample(self, elapsed_ms: u32) -> f32 {
        if !self.turning || elapsed_ms == 0 {
            return self.orientation;
        }
        let time = f64::from(seconds_from_millis(elapsed_ms));
        let full_turn = f64::from(f32::from_bits(0x40c9_0fdb));
        let angle = (self.rate * time + f64::from(self.orientation)) % full_turn;
        (if angle < 0.0 {
            angle + full_turn
        } else {
            angle
        }) as f32
    }
}

/// Absolute displacement and facing relative to a motion anchor.
#[derive(Clone, Copy, Debug)]
pub struct MovementGroundSample {
    /// Horizontal travel from the retained anchor, not the previous frame.
    pub displacement: Vec3,
    /// Facing wrapped with the original float full-turn constant.
    pub orientation: f32,
}

/// Invalid mode or scalar at the grounded analytic boundary.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("ground trajectory requires finite ground-mode state and nonnegative speeds")]
pub struct MovementGroundTrajectoryError;

impl MovementGroundTrajectory {
    /// Resolves a native ground movement flag word and speed vector.
    ///
    /// The caller owns wire/local flag conversion and excludes swimming,
    /// flight, falling, spline travel, and retained launch bases. The native
    /// secondary full-speed-turn bit is supplied separately.
    ///
    /// # Errors
    /// Rejects unsupported movement modes, invalid facing, or invalid speeds.
    pub fn new(
        flags: u32,
        full_speed_turn: bool,
        orientation: f32,
        speeds: WorldMovementSpeeds,
    ) -> Result<Self, MovementGroundTrajectoryError> {
        if flags & 0x02e0_10c0 != 0
            || !orientation.is_finite()
            || speeds.values().iter().any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(MovementGroundTrajectoryError);
        }
        let (sin, cos) = f64::from(orientation).sin_cos();
        let forward = Vec2::new(cos as f32, sin as f32);
        let mut direction = forward;
        if flags & 3 != 0 && flags & 2 != 0 {
            direction = -direction;
        }
        if flags & 0xc != 0 {
            let strafe = if flags & 4 != 0 {
                Vec2::new(-forward.y, forward.x)
            } else {
                Vec2::new(forward.y, -forward.x)
            };
            direction = if flags & 3 != 0 {
                (direction + strafe) * f32::from_bits(0x3f35_04f3)
            } else {
                strafe
            };
        }
        let speed = if flags & 0xf == 0 {
            0.0
        } else if flags & 0x100 != 0 {
            speeds.walk().min(speeds.run())
        } else if flags & 2 != 0 {
            speeds.run_back().min(speeds.run())
        } else {
            speeds.run()
        };
        let yaw =
            MovementYawTrajectory::new(flags, full_speed_turn, orientation, speeds.turn_rate())
                .map_err(|_| MovementGroundTrajectoryError)?;
        let direction_z = if flags & 2 != 0 { -0.0 } else { 0.0 };
        Ok(Self {
            basis_xy: direction,
            basis_z: direction_z,
            direction,
            speed,
            yaw,
        })
    }

    /// Returns the resolved launch direction, including the stationary basis.
    #[must_use]
    pub const fn direction(self) -> Vec2 {
        self.direction
    }

    /// Returns the unnormalized full travel basis retained by 98B850.
    #[must_use]
    pub fn travel_direction(self) -> Vec3 {
        self.basis_xy.extend(self.basis_z)
    }

    /// Changes the retained basis and facing without recomputing input or speed.
    /// The caller must convert its position anchor and preserve elapsed time.
    ///
    /// # Errors
    /// Rejects non-finite converted state before publishing the new trajectory.
    pub fn rebased(
        self,
        change: MovementTransportChange,
    ) -> Result<Self, MovementGroundTrajectoryError> {
        let direction = change.direction(self.travel_direction());
        if !direction.is_finite() {
            return Err(MovementGroundTrajectoryError);
        }
        Ok(Self {
            basis_xy: direction.truncate(),
            basis_z: direction.z,
            direction: MovementTransportChange::horizontal_direction(direction),
            yaw: self
                .yaw
                .rebased(change)
                .map_err(|_| MovementGroundTrajectoryError)?,
            ..self
        })
    }

    /// Returns the admitted movement speed.
    #[must_use]
    pub const fn speed(self) -> f32 {
        self.speed
    }

    /// Samples elapsed anchor time using the native unsigned-millisecond conversion.
    #[must_use]
    pub fn sample(self, elapsed_ms: u32) -> MovementGroundSample {
        let time = f64::from(seconds_from_millis(elapsed_ms));
        let orientation = self.yaw.sample(elapsed_ms);
        let yaw_rate = self.yaw.rate;
        let displacement = if self.speed == 0.0 || elapsed_ms == 0 {
            Vec3::ZERO
        } else if yaw_rate == 0.0 {
            (self.travel_direction().as_dvec3() * time * f64::from(self.speed)).as_vec3()
        } else {
            // Native stores the radius and trig angle before FSINCOS, then
            // retains products in x87 until the final output stores.
            let radius = f64::from((f64::from(self.speed) / yaw_rate) as f32);
            let angle = f64::from((yaw_rate * time) as f32);
            let (sin, cos) = angle.sin_cos();
            let across = f64::from(sin as f32) * radius;
            let along = radius - f64::from(cos as f32) * radius;
            let x = f64::from(self.direction.x);
            let y = f64::from(self.direction.y);
            Vec3::new(
                (x * across - y * along) as f32,
                (x * along + y * across) as f32,
                0.0,
            )
        };
        MovementGroundSample {
            displacement,
            orientation,
        }
    }
}
