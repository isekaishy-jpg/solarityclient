//! TransportPhysics bobbing and turn banking from `007F7DD0`/`007F7B30`.

use glam::Vec3;
use thiserror::Error;

use crate::movement::path::MovementPath;

/// Malformed physics values cannot enter a spatial transform.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("transport physics contains invalid wave or speed parameters")]
pub struct TransportRoutePhysicsError;

/// Validated native TransportPhysics row, excluding its record ID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportRoutePhysics {
    bob_amplitude: f64,
    bob_period_ms: Option<u32>,
    roll_amplitude: f64,
    roll_period_ms: Option<u32>,
    pitch_amplitude: f64,
    pitch_period_ms: Option<u32>,
    bank_amplitude: f64,
    maximum_turn: f64,
    maximum_speed: f64,
    idle_gain: f64,
}

impl TransportRoutePhysics {
    /// Admits the ten float columns after TransportPhysics's ID: bob amplitude
    /// and frequency, roll amplitude and frequency, pitch amplitude and frequency,
    /// bank amplitude, maximum turn, maximum speed, and idle amplitude gain.
    ///
    /// # Errors
    /// Rejects nonfinite data, invalid divisors, and frequencies whose truncated
    /// native period cannot be used by the integer remainder operation.
    pub fn new(values: [f32; 10]) -> Result<Self, TransportRoutePhysicsError> {
        if values.iter().any(|value| !value.is_finite()) || values[7] <= 0.0 || values[8] <= 0.0 {
            return Err(TransportRoutePhysicsError);
        }
        Ok(Self {
            bob_amplitude: f64::from(values[0]),
            bob_period_ms: wave_period(values[1])?,
            roll_amplitude: f64::from(values[2]),
            roll_period_ms: wave_period(values[3])?,
            pitch_amplitude: f64::from(values[4]),
            pitch_period_ms: wave_period(values[5])?,
            bank_amplitude: f64::from(values[6]),
            maximum_turn: f64::from(values[7]),
            maximum_speed: f64::from(values[8]),
            idle_gain: f64::from(values[9]),
        })
    }

    /// Native backward tangent history smooths turn rate over four samples.
    pub(super) fn apply(
        &self,
        path: &MovementPath,
        time_ms: u32,
        speed: f32,
        distance: f32,
        yaw: f32,
        position: &mut Vec3,
    ) -> (f32, f32) {
        let mut speed = f64::from(speed);
        let mut turn = 0.0;
        if speed > 0.0 {
            let lower = self.maximum_speed * f64::from(0.15_f32);
            let upper = self.maximum_speed * f64::from(0.3_f32);
            speed = if speed < lower {
                0.0
            } else if speed < upper {
                let blend = (speed - lower) / (upper - lower);
                blend * blend * speed
            } else {
                speed
            };
            speed = f64::from(speed as f32);
            let mut previous_yaw = f64::from(yaw);
            for index in 1..=4 {
                let fraction = ((f64::from(distance)
                    - f64::from(index) * speed * f64::from(0.45_f32))
                .max(0.0)
                    / f64::from(path.length()))
                .clamp(0.0, 1.0) as f32;
                let tangent = path.tangent(fraction).as_dvec3();
                let squared = tangent.length_squared();
                let tangent = if squared > f64::from(f32::EPSILON * 2.0) {
                    tangent / squared.sqrt()
                } else {
                    tangent
                };
                let angle = (-tangent.y).atan2(-tangent.x);
                let difference = previous_yaw - angle;
                let difference = if difference > f64::from(std::f32::consts::PI) {
                    difference - f64::from(std::f32::consts::TAU)
                } else if difference < -f64::from(std::f32::consts::PI) {
                    difference + f64::from(std::f32::consts::TAU)
                } else {
                    difference
                };
                turn += difference * f64::from(f32::from_bits(0x400e38e4));
                previous_yaw = f64::from(angle as f32);
                if index < 4 {
                    turn = f64::from(turn as f32);
                }
            }
            turn *= f64::from(0.2_f32);
        }
        self.wave(time_ms, speed, f64::from(turn as f32), position)
    }

    /// The three sinusoidal channels have native phase offsets 0, .5, and .2.
    fn wave(&self, time_ms: u32, speed: f64, turn: f64, position: &mut Vec3) -> (f32, f32) {
        let gain = if speed < self.maximum_speed {
            let root = (speed / self.maximum_speed).sqrt();
            (1.0 - self.idle_gain) * root * root + self.idle_gain
        } else {
            1.0
        };
        if let Some(period) = self.bob_period_ms {
            position.z = (phase(time_ms, period).sin() * self.bob_amplitude * gain
                + f64::from(position.z)) as f32;
        }
        let turn = turn.clamp(-self.maximum_turn, self.maximum_turn);
        let roll = self.roll_period_ms.map_or(0.0, |period| {
            (((phase(time_ms, period) + 0.5).sin() * self.roll_amplitude
                + (self.bank_amplitude / self.maximum_turn) * turn)
                * gain) as f32
        });
        let pitch = self.pitch_period_ms.map_or(0.0, |period| {
            ((phase(time_ms, period) + f64::from(0.2_f32)).sin() * self.pitch_amplitude * gain)
                as f32
        });
        (roll, pitch)
    }
}

/// `007F7B30` uses the original float 6283.185546875, switches x87 rounding
/// to truncation (FLDCW with 0xC00), then performs FISTP to uint64.
fn wave_period(frequency: f32) -> Result<Option<u32>, TransportRoutePhysicsError> {
    if frequency.abs() < f32::EPSILON * 2.0 {
        return Ok(None);
    }
    let period = (f64::from(f32::from_bits(0x45c4597c)) / f64::from(frequency)).trunc();
    if period < 1.0 || period > f64::from(u32::MAX) {
        return Err(TransportRoutePhysicsError);
    }
    Ok(Some(period as u32))
}

/// The original angle uses a float representation of tau in extended precision.
fn phase(time_ms: u32, period_ms: u32) -> f64 {
    f64::from(time_ms % period_ms) * f64::from(std::f32::consts::TAU) / f64::from(period_ms)
}
