//! Scene-time ripple growth and opacity from 0x0079CF40 and 0x0079D5E0.

use glam::Vec3;
use thiserror::Error;

use super::emission::WaterRippleEmission;

/// A retained ripple's scalar lifetime, independent of its frozen surface mesh.
#[derive(Clone, Copy, Debug)]
pub struct WaterRippleEnvelope {
    position: Vec3,
    yaw: f32,
    radius: f32,
    growth: f32,
    opacity: f32,
    peak: f32,
    rise: f32,
    fall: f32,
    expires_at: f32,
    directional: bool,
    bounds: [Vec3; 2],
    alive: bool,
}

/// Invalid input to the scene-time ripple envelope.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error(
    "water ripple envelope requires finite scalars, positive radius and lifetime, and nonnegative strength, growth and time"
)]
pub struct WaterRippleEnvelopeError;

impl WaterRippleEnvelope {
    /// Normalizes a unit request at 0x0079D460, then initializes its record at
    /// 0x0079CF40. The returned bounds select water triangles once at emission.
    ///
    /// # Errors
    /// Rejects invalid requests and arithmetic overflow before retaining state.
    pub fn new(
        emission: WaterRippleEmission,
        scene_time: f32,
    ) -> Result<Self, WaterRippleEnvelopeError> {
        if !emission.position.is_finite()
            || !emission.yaw.is_finite()
            || [emission.radius, emission.lifetime]
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.)
            || [emission.strength, emission.growth, scene_time]
                .iter()
                .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(WaterRippleEnvelopeError);
        }
        let peak = if emission.strength < f32::from_bits(0x3e2a_aaab) {
            emission.strength * 6.
        } else {
            1.
        };
        let lifetime = f64::from(emission.lifetime);
        // The end radius remains in x87 across growth and bounds calculations.
        let end_radius = f64::from(emission.growth) * lifetime + f64::from(emission.radius);
        let growth = ((end_radius - f64::from(emission.radius)) / lifetime) as f32;
        let rise_fraction = f64::from(0.4_f32); // 0xA3FAC0 and 0xA3FAC4 both store 0.4.
        let rise = (f64::from(peak) / (rise_fraction * lifetime)) as f32;
        let fall = -(f64::from(peak) / ((1. - rise_fraction) * lifetime)) as f32;
        let expires_at = scene_time + emission.lifetime;
        let bounds = [-1., 1.].map(|sign| {
            Vec3::new(
                (f64::from(emission.position.x) + sign * end_radius) as f32,
                (f64::from(emission.position.y) + sign * end_radius) as f32,
                (f64::from(emission.position.z) + sign) as f32,
            )
        });
        if [growth, rise, fall, expires_at]
            .iter()
            .any(|value| !value.is_finite())
            || bounds.iter().any(|bound| !bound.is_finite())
        {
            return Err(WaterRippleEnvelopeError);
        }
        Ok(Self {
            position: emission.position,
            yaw: -emission.yaw, // 0x0079D180 negates the request before projection.
            radius: emission.radius,
            growth,
            opacity: 0.,
            peak,
            rise,
            fall,
            expires_at,
            directional: emission.directional,
            bounds,
            alive: true,
        })
    }

    /// Advances the original envelope once for a rendered scene frame. Expiry
    /// and nonpositive opacity retire the ripple permanently, including dt=0
    /// immediately after emission, as 0x0079D5E0 does.
    ///
    /// # Errors
    /// Rejects negative or nonfinite frame times without changing the envelope.
    pub fn advance(
        &mut self,
        elapsed: f32,
        scene_time: f32,
    ) -> Result<bool, WaterRippleEnvelopeError> {
        if [elapsed, scene_time]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(WaterRippleEnvelopeError);
        }
        if !self.alive || self.expires_at <= scene_time {
            self.alive = false;
            return Ok(false);
        }
        let mut fade_elapsed = f64::from(elapsed);
        self.radius = (f64::from(self.growth) * fade_elapsed + f64::from(self.radius)) as f32;
        if self.rise != 0. {
            let opacity = fade_elapsed * f64::from(self.rise) + f64::from(self.opacity);
            self.opacity = opacity as f32;
            if opacity > f64::from(self.peak) {
                // Stock carries the consumed rise duration into the falling
                // branch, including this crossing frame. Preserve that choice.
                fade_elapsed -= (opacity - f64::from(self.peak)) / f64::from(self.rise);
                self.opacity = self.peak;
                self.rise = 0.;
            }
        }
        if self.rise == 0. {
            self.opacity = (fade_elapsed * f64::from(self.fall) + f64::from(self.opacity)) as f32;
        }
        self.alive = self.opacity > 0.;
        Ok(self.alive)
    }

    /// Frozen world center used by the projection matrix.
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Negated emission yaw used by native texture projection.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Current projected radius in world units.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Current vertex alpha before texture alpha multiplication.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Selects the authored directional wake texture.
    pub fn directional(&self) -> bool {
        self.directional
    }

    /// Original emission bounds for collecting the retained water triangles.
    pub fn surface_bounds(&self) -> [Vec3; 2] {
        self.bounds
    }
}
