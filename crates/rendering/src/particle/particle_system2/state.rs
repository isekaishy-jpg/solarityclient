//! Placement-local particle lifetime and ballistic state.

use glam::Vec3;
use thiserror::Error;

/// Exact positive lower lifetime bound loaded at executable `0x009E1134`.
const MINIMUM_LIFETIME_SECONDS: f32 = f32::from_bits(0x3a83_126f);

/// Exact signed-short normalization loaded at executable `0x009EA0B4`.
const SIGNED_SHORT_SCALE: f32 = f32::from_bits(0x3800_0100);

/// Velocity components below this executable epsilon are snapped to zero.
const VELOCITY_EPSILON: f32 = f32::from_bits(0x322b_cc77);

/// Mutable 32-byte stock particle prefix used by ordinary billboard particles.
///
/// Build 12340 appends tumbling and geometry-particle state for specialized
/// paths. This prefix remains common: age, position, velocity, and one random
/// word shared by lifetime variation and atlas selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticleState {
    age_seconds: f32,
    position: Vec3,
    velocity: Vec3,
    random_word: u16,
}

// Twinkle phases shift stock's particle pointer by five; preserve the same
// pool stride even though fields remain private and serialization-independent.
const _: () = assert!(std::mem::size_of::<M2ParticleState>() == 32);

impl M2ParticleState {
    /// Creates the common state written by one stock emitter spawn.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleStateError`] when age, position, or velocity cannot
    /// participate in the finite simulation domain.
    pub fn new(
        age_seconds: f32,
        position: Vec3,
        velocity: Vec3,
        random_word: u16,
    ) -> Result<Self, M2ParticleStateError> {
        if !age_seconds.is_finite() || age_seconds < 0.0 {
            return Err(M2ParticleStateError::Age);
        }
        if !position.is_finite() {
            return Err(M2ParticleStateError::Position);
        }
        if !velocity.is_finite() {
            return Err(M2ParticleStateError::Velocity);
        }
        Ok(Self {
            age_seconds,
            position,
            velocity,
            random_word,
        })
    }

    /// Advances stock ballistic motion, gravity, and clamped linear drag.
    ///
    /// Build 12340 increments age before evaluating particle death. The caller
    /// can therefore call [`Self::is_alive`] immediately after this method and
    /// remove the particle before preparing render data.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleStateError::ElapsedTime`] for a negative or
    /// non-finite update and [`M2ParticleStateError::Forces`] when gravity or
    /// drag is non-finite.
    pub fn advance(
        &mut self,
        elapsed_seconds: f32,
        gravity: f32,
        drag: f32,
    ) -> Result<(), M2ParticleStateError> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return Err(M2ParticleStateError::ElapsedTime);
        }
        if !gravity.is_finite() || !drag.is_finite() {
            return Err(M2ParticleStateError::Forces);
        }
        self.age_seconds += elapsed_seconds;
        self.advance_motion(elapsed_seconds, gravity, drag);
        Ok(())
    }

    /// Applies one birth-frame ballistic step without aging the newborn.
    ///
    /// Stock scatters the initial age inside the current slice, then applies
    /// that slice's motion after emission. The ordinary old-particle pass has
    /// already run, so adding the full slice to age here would age every
    /// newborn twice.
    pub(super) fn advance_newborn_motion(&mut self, elapsed_seconds: f32, gravity: f32, drag: f32) {
        self.advance_motion(elapsed_seconds, gravity, drag);
    }

    fn advance_motion(&mut self, elapsed_seconds: f32, gravity: f32, drag: f32) {
        self.position += self.velocity * elapsed_seconds;
        self.position.z -= gravity * elapsed_seconds * elapsed_seconds * 0.5;
        self.velocity.z -= gravity * elapsed_seconds;
        if drag != 0.0 {
            let amount = (elapsed_seconds * drag).min(1.0);
            self.velocity -= self.velocity * amount;
        }
        self.velocity = self.velocity.map(|component| {
            if component != 0.0 && component.abs() < VELOCITY_EPSILON {
                0.0
            } else {
                component
            }
        });
    }

    /// Returns the stock lifetime selected by this particle's random word.
    #[must_use]
    pub fn lifetime_seconds(self, base_seconds: f32, variation: f32) -> f32 {
        let signed_random = self.random_word as i16;
        (base_seconds + f32::from(signed_random) * variation * SIGNED_SHORT_SCALE)
            .max(MINIMUM_LIFETIME_SECONDS)
    }

    /// Reports whether age remains strictly below the selected lifetime.
    #[must_use]
    pub fn is_alive(self, base_seconds: f32, variation: f32) -> bool {
        self.age_seconds < self.lifetime_seconds(base_seconds, variation)
    }

    /// Returns seconds elapsed since emission.
    #[must_use]
    pub const fn age_seconds(self) -> f32 {
        self.age_seconds
    }

    /// Returns age divided by the selected lifetime.
    #[must_use]
    pub fn normalized_age(self, base_seconds: f32, variation: f32) -> f32 {
        self.age_seconds / self.lifetime_seconds(base_seconds, variation)
    }

    /// Returns the current particle position in its authored simulation space.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns current linear velocity in the same simulation space.
    #[must_use]
    pub const fn velocity(self) -> Vec3 {
        self.velocity
    }

    /// Returns the stored random word shared by variation and atlas selection.
    #[must_use]
    pub const fn random_word(self) -> u16 {
        self.random_word
    }

    /// Adds an authored acceleration impulse before ordinary ballistic motion.
    pub(super) fn add_velocity(&mut self, impulse: Vec3) {
        self.velocity += impulse;
    }

    /// Applies the emitter's recovered follow-position displacement.
    pub(super) fn translate(&mut self, displacement: Vec3) {
        self.position += displacement;
    }
}

/// Invalid input cannot participate in stock particle simulation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2ParticleStateError {
    /// Initial age must be finite and nonnegative.
    #[error("particle age must be finite and nonnegative")]
    Age,
    /// Initial position must contain finite components.
    #[error("particle position must be finite")]
    Position,
    /// Initial velocity must contain finite components.
    #[error("particle velocity must be finite")]
    Velocity,
    /// Elapsed time must be finite and nonnegative.
    #[error("particle elapsed time must be finite and nonnegative")]
    ElapsedTime,
    /// Gravity and drag must be finite.
    #[error("particle forces must be finite")]
    Forces,
}
