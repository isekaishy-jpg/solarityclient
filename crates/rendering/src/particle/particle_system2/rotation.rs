//! Particle-local initial rotation and angular-velocity selection.

use solarity_asset::M2ParticleEmitter;

use super::M2ParticleRandom;

/// Gives each particle an independent chance to reverse angular velocity.
const NEGATE_SPIN_RANDOM: u32 = 0x0000_0200;

/// Rotation parameters selected once from a particle's stored random word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticleRotationPose {
    initial_radians: f32,
    radians_per_second: f32,
}

impl M2ParticleRotationPose {
    /// Reproduces executable `0x0097A130` and its conditional call order.
    ///
    /// Initial-angle and angular-velocity variation use a fresh generator
    /// seeded from the same particle word as appearance sampling. A zero
    /// variation skips its draw instead of consuming an unused value.
    #[must_use]
    pub fn sample(emitter: &M2ParticleEmitter, random_word: u16) -> Self {
        let mut random = M2ParticleRandom::new(u32::from(random_word));
        let initial_radians = varied(
            emitter.base_spin(),
            emitter.base_spin_variation(),
            &mut random,
        );
        let mut radians_per_second = varied(
            emitter.spin_speed(),
            emitter.spin_speed_variation(),
            &mut random,
        );
        if emitter.flags() & NEGATE_SPIN_RANDOM != 0 && random.next_unit() < 0.5 {
            radians_per_second = -radians_per_second;
        }
        Self {
            initial_radians,
            radians_per_second,
        }
    }

    /// Returns the particle's angle at its current age.
    #[must_use]
    pub fn angle_radians(self, age_seconds: f32) -> f32 {
        age_seconds * self.radians_per_second + self.initial_radians
    }

    /// Returns the selected initial quad rotation.
    #[must_use]
    pub const fn initial_radians(self) -> f32 {
        self.initial_radians
    }

    /// Returns the selected angular velocity.
    #[must_use]
    pub const fn radians_per_second(self) -> f32 {
        self.radians_per_second
    }
}

/// Applies one optional signed-random variation without changing call order.
fn varied(base: f32, variation: f32, random: &mut M2ParticleRandom) -> f32 {
    if variation == 0.0 {
        base
    } else {
        base + random.next_signed() * variation
    }
}
