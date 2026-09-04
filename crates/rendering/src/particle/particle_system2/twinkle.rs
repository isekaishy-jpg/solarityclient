//! Process-wide particle twinkle phases recovered from build 12340.

use solarity_asset::M2ParticleEmitter;
use thiserror::Error;

use super::{M2ParticleRandom, M2ParticleState};

/// Number of float phases initialized at executable address `0x00DCE690`.
const TWINKLE_PHASE_COUNT: usize = 128;

/// Process-wide random values shared by every M2 particle emitter.
///
/// Stock initializes this table once from two Visual C++ `rand` calls during
/// `M2Initialize`. Emitters subsequently select a phase from their current
/// 32-byte pool address, so the table belongs above any placement or emitter.
#[derive(Clone, Debug, PartialEq)]
pub struct M2ParticleTwinkleTable {
    phases: [f32; TWINKLE_PHASE_COUNT],
}

impl M2ParticleTwinkleTable {
    /// Builds the exact 128-value table from the combined stock seed.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut random = M2ParticleRandom::new(seed);
        Self {
            phases: std::array::from_fn(|_index| random.next_unit()),
        }
    }

    /// Selects visibility and scale for one particle at its current pool slot.
    ///
    /// `Some(scale)` means the particle remains visible. `None` reproduces the
    /// stock early return when its random phase exceeds `twinkle_percent`.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleTwinkleError`] if age multiplied by the authored
    /// speed cannot enter stock's signed 32-bit nearest-even phase counter.
    pub fn sample(
        &self,
        emitter: &M2ParticleEmitter,
        particle: &M2ParticleState,
    ) -> Result<Option<f32>, M2ParticleTwinkleError> {
        let scale = emitter.twinkle_scale();

        let counter = (emitter.twinkle_speed() * particle.age_seconds()).round_ties_even();
        if !counter.is_finite() || counter < -2_147_483_648.0 || counter >= 2_147_483_648.0 {
            return Err(M2ParticleTwinkleError::PhaseCounter);
        }
        // The original shifts the address of the fixed 32-byte particle pool
        // slot. The active-index list may swap-remove, but a surviving
        // particle's slot (and therefore its presentation phase) does not.
        let pool_slot = u32::from(particle.pool_address_phase());
        let phase_index =
            pool_slot.wrapping_add(counter as i32 as u32) & (TWINKLE_PHASE_COUNT as u32 - 1);
        let phase = self.phases[phase_index as usize];
        if emitter.twinkle_percent() < phase {
            return Ok(None);
        }
        // `FUN_0097A670` reads the two authored floats at emitter offsets
        // 0x144 and 0x148 as a base and an additive random range. The second
        // component is not a maximum endpoint.
        Ok(Some(scale.x + phase * scale.y))
    }

    /// Returns one generated phase for executable-vector verification.
    #[must_use]
    pub fn phase(&self, index: u8) -> Option<f32> {
        self.phases.get(index as usize).copied()
    }
}

/// Authored twinkle state cannot enter the stock phase calculation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2ParticleTwinkleError {
    /// The nearest-even twinkle counter must fit stock's signed integer.
    #[error("particle twinkle phase counter exceeds the signed 32-bit domain")]
    PhaseCounter,
}
