//! Reversible linear sound envelopes from SoundEngine.cpp at 0x0087a425.

use super::SoundGain;
use std::time::Duration;

/// Direction selected by the original voice's two mutually exclusive fade flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundFadeDirection {
    /// Increase the current envelope toward one.
    In,
    /// Decrease the current envelope toward zero, then retire the voice.
    Out,
}

/// One native voice envelope, independent of source, spatial, and category gain.
#[derive(Clone, Copy, Debug)]
pub struct SoundFade {
    gain: f32,
    direction: Option<SoundFadeDirection>,
    duration_seconds: f32,
}

impl Default for SoundFade {
    fn default() -> Self {
        Self {
            gain: 1.0,
            direction: None,
            duration_seconds: 0.0,
        }
    }
}

impl SoundFade {
    /// Creates an envelope at an explicit initial gain, without advancing time.
    pub const fn new(gain: SoundGain) -> Self {
        Self {
            gain: gain.value(),
            direction: None,
            duration_seconds: 0.0,
        }
    }
    /// Returns the current multiplier before other mixer policies are composed.
    pub const fn gain(self) -> f32 {
        self.gain
    }
    /// Changes direction without resetting progress, as when returning to a zone.
    pub fn retarget(&mut self, direction: SoundFadeDirection, duration: Duration) {
        self.retarget_seconds(direction, duration.as_secs_f32());
    }
    /// Retains the original single-precision duration supplied by a Lua command.
    /// Unlike a Rust duration, the native field can also contain a negative or
    /// infinite value; command owners decide whether a negative means default.
    pub fn retarget_seconds(&mut self, direction: SoundFadeDirection, seconds: f32) {
        self.direction = Some(direction);
        self.duration_seconds = seconds;
    }
    /// Advances by elapsed real time; returns true when a fade-out retires a voice.
    /// Native slope is 1/duration, so reversing a partial fade preserves its gain.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        let Some(direction) = self.direction else {
            return false;
        };
        let delta = if self.duration_seconds == 0.0 {
            f64::INFINITY
        } else {
            f64::from(elapsed.as_secs_f32()) / f64::from(self.duration_seconds)
        };
        match direction {
            SoundFadeDirection::In => {
                self.gain = (f64::from(self.gain) + delta).min(1.0) as f32;
                if self.gain >= 1.0 {
                    self.direction = None;
                }
                false
            }
            SoundFadeDirection::Out => {
                self.gain = (f64::from(self.gain) - delta).max(0.0) as f32;
                self.gain <= 0.0
            }
        }
    }
}
