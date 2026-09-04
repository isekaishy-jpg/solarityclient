//! Immutable model-animation values sampled for one particle update.

use glam::Vec3;
use solarity_asset::{M2AnimationSet, M2ParticleEmitter, M2ParticleGravity};

use crate::model::m2_animation::sample::{sample_discrete, sample_scalar, sample_vec3};
use crate::{M2AnimationClock, M2BonePoseError};

/// One animation-clock sample of a build-12340 particle emitter.
///
/// Particle lifetime ramps are deliberately absent. They use each particle's
/// normalized age and are sampled by [`super::M2ParticleLifetimePose`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticlePose {
    emission_speed: f32,
    speed_variation: f32,
    vertical_range: f32,
    horizontal_range: f32,
    gravity: Vec3,
    lifespan: f32,
    emission_rate: f32,
    emission_area_width: f32,
    emission_area_length: f32,
    z_source: f32,
    enabled: bool,
}

impl M2ParticlePose {
    /// Samples all ordinary M2 tracks used by an emitter update.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] when the selected animation sequence or
    /// either animation clock is unavailable to the decoded model.
    pub fn sample(
        animations: &M2AnimationSet,
        emitter: &M2ParticleEmitter,
        clock: M2AnimationClock,
    ) -> Result<Self, M2BonePoseError> {
        let sequence = clock.resolve(animations)?;
        let animation_time_ms = clock.animation_time_ms();
        let global_time_ms = clock.global_time_ms();
        let scalar = |track, default| {
            sample_scalar(
                animations,
                track,
                sequence,
                animation_time_ms,
                global_time_ms,
                default,
            )
        };
        Ok(Self {
            emission_speed: scalar(emitter.emission_speed(), 0.0),
            speed_variation: scalar(emitter.speed_variation(), 0.0),
            vertical_range: scalar(emitter.vertical_range(), 0.0),
            horizontal_range: scalar(emitter.horizontal_range(), 0.0),
            gravity: match emitter.gravity() {
                M2ParticleGravity::Scalar(track) => Vec3::NEG_Z * scalar(track, 0.0),
                M2ParticleGravity::Compressed(track) => sample_vec3(
                    animations,
                    track,
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                    Vec3::ZERO,
                ),
            },
            lifespan: scalar(emitter.lifespan(), 0.0),
            emission_rate: scalar(emitter.emission_rate(), 0.0),
            emission_area_width: scalar(emitter.emission_area_width(), 0.0),
            emission_area_length: scalar(emitter.emission_area_length(), 0.0),
            z_source: scalar(emitter.z_source(), 0.0),
            enabled: sample_discrete(
                animations,
                emitter.enabled(),
                sequence,
                animation_time_ms,
                global_time_ms,
                1,
            ) != 0,
        })
    }

    /// Returns the animated base launch speed.
    #[must_use]
    pub const fn emission_speed(self) -> f32 {
        self.emission_speed
    }

    /// Returns the animated signed-random launch-speed multiplier.
    #[must_use]
    pub const fn speed_variation(self) -> f32 {
        self.speed_variation
    }

    /// Returns the maximum polar launch angle in radians.
    #[must_use]
    pub const fn vertical_range(self) -> f32 {
        self.vertical_range
    }

    /// Returns the maximum azimuth launch angle in radians.
    #[must_use]
    pub const fn horizontal_range(self) -> f32 {
        self.horizontal_range
    }

    /// Returns the animated local acceleration vector.
    #[must_use]
    pub const fn gravity(self) -> Vec3 {
        self.gravity
    }

    /// Returns the animated base lifetime in seconds.
    #[must_use]
    pub const fn lifespan(self) -> f32 {
        self.lifespan
    }

    /// Returns the animated base emission rate in particles per second.
    #[must_use]
    pub const fn emission_rate(self) -> f32 {
        self.emission_rate
    }

    /// Returns the animated plane width or spherical maximum radius.
    #[must_use]
    pub const fn emission_area_width(self) -> f32 {
        self.emission_area_width
    }

    /// Returns the animated plane length or spherical minimum radius.
    #[must_use]
    pub const fn emission_area_length(self) -> f32 {
        self.emission_area_length
    }

    /// Returns the animated source height used to aim initial velocity.
    #[must_use]
    pub const fn z_source(self) -> f32 {
        self.z_source
    }

    /// Reports whether the emitter's held enable key is nonzero.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}
