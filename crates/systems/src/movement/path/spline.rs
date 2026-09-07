//! Retained spline clocks and unit transforms from `0098CA00`/`006EB0B0`.

use glam::Vec3;
use shipyard::Component;
use solarity_ecs::{WorldMovementSpline, WorldMovementState, WorldTransform};
use thiserror::Error;

use super::{MovementPath, MovementPathError, MovementPathMode};
use crate::movement::fall::{MovementFallError, MovementFallMode, MovementFallTrajectory};

/// Final orientation request applied once the path reaches its destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementSplineFacing {
    /// Preserve the evaluated path direction.
    Direction,
    /// Use the supplied angle in radians.
    Angle(f32),
    /// Resolve the current position of this GUID at completion.
    Target(u64),
    /// Face the supplied point in path coordinates.
    Point(Vec3),
}

impl MovementSplineFacing {
    /// Resolves endpoint facing using the target position available at completion.
    #[must_use]
    pub fn resolve(
        self,
        current: WorldTransform,
        target_position: impl FnOnce(u64) -> Option<Vec3>,
    ) -> f32 {
        match self {
            Self::Direction => current.orientation(),
            Self::Angle(angle) => angle,
            Self::Point(point) => facing_angle(current.position(), point),
            Self::Target(guid) => target_position(guid).map_or(current.orientation(), |point| {
                facing_angle(current.position(), point)
            }),
        }
    }
}

/// Complete movement-owner input, independent of the packet representation.
#[derive(Clone, Debug)]
pub struct MovementSplineDefinition {
    /// Exact native spline flags.
    pub flags: u32,
    /// Authored endpoint orientation.
    pub facing: MovementSplineFacing,
    /// Authored path identity.
    pub id: u32,
    /// Path time already consumed when the snapshot was sent.
    pub elapsed_ms: u32,
    /// Authored traversal duration in milliseconds.
    pub duration_ms: u32,
    /// Current-cycle duration multiplier.
    pub duration_scale: f32,
    /// Multiplier applied on the next cycle; later cycles use one.
    pub next_duration_scale: f32,
    /// Parabolic acceleration in yards per second squared.
    pub vertical_acceleration: f32,
    /// Delay before the parabolic or animation effect, in milliseconds.
    pub effect_start_ms: u32,
    /// Geometry includes the native endpoint controls.
    pub nodes: Vec<Vec3>,
    /// Separately supplied native final position.
    pub destination: Vec3,
}

/// Failure admitting or evaluating a native path safely.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovementSplineError {
    /// Control geometry cannot be evaluated safely.
    #[error(transparent)]
    Path(#[from] MovementPathError),
    /// The analytic vertical trajectory failed.
    #[error(transparent)]
    Fall(#[from] MovementFallError),
    /// Native assumes finite, nonnegative duration scales and finite positions.
    #[error("movement spline contains invalid timing or placement")]
    InvalidDefinition,
}

/// Live path owner; replacing or deleting its entity drops all path storage.
#[derive(Clone, Debug, Component)]
pub struct MovementSpline {
    path: MovementPath,
    definition: MovementSplineDefinition,
    start_ms: u32,
    elapsed_ms: u32,
    direction: Vec3,
    fall_origin_z: f32,
    effect_started: bool,
}

impl MovementSpline {
    /// Advances placement and publishes native completion/effect flag changes.
    ///
    /// # Errors
    /// Returns the path's geometry or analytic fall failure.
    pub fn advance_movement(
        &mut self,
        now_ms: u32,
        movement: WorldMovementState,
        current: WorldTransform,
        target_position: impl FnOnce(u64) -> Option<Vec3>,
    ) -> Result<(WorldTransform, WorldMovementState), MovementSplineError> {
        let transform = self.advance(now_ms, movement.flags(), current, target_position)?;
        let summary = self.motion();
        let mut flags = movement.flags();
        if summary.effect_started {
            if summary.flags & 0x200000 != 0 {
                flags |= 0x0100_0000_0000;
            } else if summary.flags & 0x800 != 0 {
                flags |= 0x0080_0000_0000;
            }
        }
        if summary.flags & 0x400 != 0 {
            // 0098BD10 releases path movement; 006EAE70 ends parabolic effects.
            flags &= !0x0080_0000_4003;
            if summary.flags & 0xa00 != 0 {
                flags &= !0x3000;
                if flags & 0x100000 != 0 {
                    flags = (flags & !0x00dfc0ff) | 0x800;
                }
            }
        }
        Ok((transform, movement.with_flags(flags).with_spline(summary)))
    }

    /// Installs a creation snapshot using `receipt - elapsed` (`004F4B50`).
    /// Native installation derives geometry mode from flags, not the wire byte.
    ///
    /// # Errors
    /// Rejects malformed geometry or timing without synthesizing a path.
    pub fn new(
        mut definition: MovementSplineDefinition,
        receipt_ms: u32,
        transform: WorldTransform,
    ) -> Result<Self, MovementSplineError> {
        if !definition.destination.is_finite()
            || !transform.position().is_finite()
            || !transform.orientation().is_finite()
            || !definition.duration_scale.is_finite()
            || definition.duration_scale < 0.0
            || !definition.next_duration_scale.is_finite()
            || definition.next_duration_scale < 0.0
            || !definition.vertical_acceleration.is_finite()
            || match definition.facing {
                MovementSplineFacing::Angle(angle) => !angle.is_finite(),
                MovementSplineFacing::Point(point) => !point.is_finite(),
                _ => false,
            }
        {
            return Err(MovementSplineError::InvalidDefinition);
        }
        let mode = if definition.flags & 0x42000 == 0 {
            MovementPathMode::Linear
        } else {
            MovementPathMode::Smooth
        };
        let path = MovementPath::new(std::mem::take(&mut definition.nodes), mode)?;
        let start_ms = receipt_ms.wrapping_sub(definition.elapsed_ms);
        Ok(Self {
            elapsed_ms: definition.elapsed_ms,
            definition,
            path,
            start_ms,
            direction: Vec3::new(
                transform.orientation().cos(),
                transform.orientation().sin(),
                0.0,
            ),
            fall_origin_z: transform.position().z,
            effect_started: false,
        })
    }

    /// Compact state consumed by animation and sound, without copying geometry.
    #[must_use]
    pub fn motion(&self) -> WorldMovementSpline {
        WorldMovementSpline {
            flags: self.definition.flags,
            length: self.path.length(),
            duration_ms: self.definition.duration_ms,
            effect_started: self.effect_started,
        }
    }

    /// Returns the authored path identity.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.definition.id
    }

    /// Evaluates the clock once and applies native endpoint/facing completion.
    /// Target lookup receives a GUID and must return a point in path coordinates.
    ///
    /// # Errors
    /// Reports invalid geometry after an initial-cycle conversion or an invalid
    /// analytic fall result. No replacement trajectory is generated.
    pub fn advance(
        &mut self,
        now_ms: u32,
        movement_flags: u64,
        current: WorldTransform,
        target_position: impl FnOnce(u64) -> Option<Vec3>,
    ) -> Result<WorldTransform, MovementSplineError> {
        self.effect_started |= self.definition.flags & 0x800 != 0
            && movement_flags & 0x0080_0000_0000 != 0
            || self.definition.flags & 0x200000 != 0 && movement_flags & 0x0100_0000_0000 != 0;
        if self.definition.flags & 0x400 != 0
            || movement_flags & 3 == 0
            || self.definition.flags & 0x400000 != 0
        {
            return Ok(current);
        }
        let fraction = self.fraction(now_ms)?;
        let sample = self.path.sample(fraction, self.direction);
        self.direction = sample.direction;
        let mut orientation = current.orientation();
        if self.definition.flags & 0x4200 == 0
            && sample.direction.as_dvec3().truncate().length_squared() > f64::from(0.001849_f32)
        {
            orientation = f64::from(sample.direction.y).atan2(f64::from(sample.direction.x)) as f32;
        }
        if self.definition.flags & 0x08000000 != 0 {
            orientation -= std::f32::consts::PI;
        }
        if orientation < 0.0 {
            orientation += std::f32::consts::TAU;
        }
        let mut position = sample.position;
        self.apply_vertical(&mut position)?;
        if self.definition.flags & 0x100 != 0 {
            position = self.definition.destination;
            orientation = self
                .definition
                .facing
                .resolve(WorldTransform::new(position, orientation), target_position);
            self.definition.flags |= 0x400;
        }
        Ok(WorldTransform::new(position, orientation))
    }

    /// Native `0098CA00` consumes at most one cycle per evaluation, including
    /// long frame gaps; modulo would skip the authored first-cycle transition.
    fn fraction(&mut self, now_ms: u32) -> Result<f32, MovementSplineError> {
        self.elapsed_ms = now_ms.wrapping_sub(self.start_ms);
        let duration = scaled_duration(self.definition.duration_ms, self.definition.duration_scale);
        if duration == 0 {
            self.definition.flags |= 0x100;
            return Ok(1.0);
        }
        if (self.elapsed_ms as i32) < 0 {
            return Ok(0.0);
        }
        if self.elapsed_ms < duration {
            return Ok((f64::from(self.elapsed_ms) / f64::from(duration)) as f32);
        }
        if self.definition.flags & 0x80000 == 0 {
            self.definition.flags |= 0x100;
            return Ok(1.0);
        }
        self.elapsed_ms -= duration;
        self.start_ms = now_ms.wrapping_sub(self.elapsed_ms);
        if self.definition.flags & 0x100000 != 0 {
            self.path.enter_cycle()?;
            self.definition.flags &= !0x100000;
            // 0098C940 replaces the native spline through 0098C770.
            self.elapsed_ms = 0;
            self.definition.next_duration_scale = 1.0;
            self.definition.effect_start_ms = 0;
            self.definition.vertical_acceleration = 0.0;
        }
        self.definition.duration_scale = self.definition.next_duration_scale;
        self.definition.next_duration_scale = 1.0;
        // The loop branch stores the scaled duration as float before 0048BCF0.
        let duration = (f64::from(
            (f64::from(self.definition.duration_ms) * f64::from(self.definition.duration_scale))
                as f32,
        ) + 0.5) as u32;
        Ok(if duration == 0 {
            1.0
        } else {
            (f64::from(self.elapsed_ms) / f64::from(duration)) as f32
        })
    }

    /// Vertical offset and airborne effect admission from `00987D20`/`0098CA00`.
    fn apply_vertical(&mut self, position: &mut Vec3) -> Result<(), MovementSplineError> {
        let flags = self.definition.flags;
        if flags & 0x200800 != 0 && self.elapsed_ms > self.definition.effect_start_ms {
            self.effect_started = true;
        }
        if flags & 0x200000 != 0
            || self.elapsed_ms == 0
            || self.elapsed_ms >= self.definition.duration_ms
        {
            return Ok(());
        }
        if flags & 0x800 != 0 {
            if self.elapsed_ms > self.definition.effect_start_ms {
                let seconds_per_ms = f64::from(0.001_f32);
                let start = f64::from(self.definition.effect_start_ms) * seconds_per_ms;
                let elapsed = f64::from(self.elapsed_ms) * seconds_per_ms - start;
                let duration = f64::from(self.definition.duration_ms) * seconds_per_ms - start;
                let acceleration = f64::from(self.definition.vertical_acceleration);
                position.z = (f64::from(position.z) + duration * acceleration * 0.5 * elapsed
                    - acceleration * elapsed * elapsed * 0.5) as f32;
            }
        } else if flags & 0x200 != 0 {
            let fall = MovementFallTrajectory::new(MovementFallMode::Normal, 0.0)?;
            let distance = fall.distance_at_seconds(self.elapsed_ms as f32 * 0.001_f32)?;
            position.z = (self.fall_origin_z - distance).max(self.definition.destination.z);
            if position.z == self.definition.destination.z {
                self.definition.flags |= 0x100;
            }
        }
        Ok(())
    }
}

/// Native duration multiplier and nearest-integer conversion (`0098CA00`).
fn scaled_duration(duration_ms: u32, scale: f32) -> u32 {
    (f64::from(duration_ms) * f64::from(scale) + 0.5) as u32
}

/// `004F5130` preserves the stock axis tolerances, including coincident points.
fn facing_angle(position: Vec3, target: Vec3) -> f32 {
    let delta = target.as_dvec3() - position.as_dvec3();
    let epsilon = f64::from(f32::EPSILON * 2.0);
    if delta.x.abs() < epsilon {
        return if delta.y < 0.0 { 1.5 } else { 0.5 } * std::f32::consts::PI;
    }
    if delta.y.abs() >= epsilon {
        return delta.y.atan2(delta.x) as f32;
    }
    if target.x < position.x {
        std::f32::consts::PI
    } else {
        0.0
    }
}
