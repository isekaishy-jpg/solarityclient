//! Shared stock live-growth and immutable authored-capacity bounds.

use super::super::M2ParticlePose;
use super::{M2ParticleSimulation, M2ParticleSimulationError};
use solarity_asset::{M2ParticleEmitter, M2Track};

/// Executable constant at `0x009F23CC` used to overprovision emitter storage.
const STOCK_CAPACITY_HEADROOM: f64 = f32::from_bits(0x3F93_3333) as f64;

impl M2ParticleSimulation {
    /// Reserves the largest pool implied by an emitter's authored rate and
    /// lifetime keys before its first visible update.
    ///
    /// Stock grows to the same estimate as animated values are encountered.
    /// Glue scenes are a finite, persistent presentation set, so reserving the
    /// maximum up front preserves emission while preventing repeated unified
    /// GPU-buffer replacement as the login animation ramps its rates.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleSimulationError::Capacity`] for non-finite or
    /// unrepresentable authored bounds, or `Allocation` when reservation fails.
    pub fn reserve_authored_capacity(
        &mut self,
        emitter: &M2ParticleEmitter,
    ) -> Result<(), M2ParticleSimulationError> {
        let required = Self::authored_capacity(emitter)?;
        if required > self.capacity {
            self.reserve_particle_storage(required)?;
        }
        Ok(())
    }

    /// Bounds the stock rate/lifetime estimate without changing live simulation.
    /// Step/linear track maxima and the existing headroom rule bound future growth.
    /// Callers cache this immutable result with the emitter resource.
    /// # Errors
    /// Non-finite or unrepresentable authored capacity is rejected.
    pub fn authored_capacity(
        emitter: &M2ParticleEmitter,
    ) -> Result<usize, M2ParticleSimulationError> {
        let maximum_rate = maximum_authored_value(emitter.emission_rate())
            + f64::from(emitter.emission_rate_variation());
        let maximum_lifetime =
            maximum_authored_value(emitter.lifespan()) + f64::from(emitter.lifespan_variation());
        let estimate = maximum_rate * maximum_lifetime * STOCK_CAPACITY_HEADROOM;
        if !estimate.is_finite() || estimate < 0.0 || estimate > usize::MAX as f64 {
            return Err(M2ParticleSimulationError::Capacity);
        }
        Ok(estimate.ceil() as usize)
    }

    /// Grows the particle pool using build 12340's current-rate estimate.
    ///
    /// `0x0097EDF0` evaluates the float inputs in x87 extended precision,
    /// sets the control word's RC bits to truncate before FISTP, and only
    /// reallocates when the estimate grows.
    pub(super) fn grow_stock_capacity(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
    ) -> Result<(), M2ParticleSimulationError> {
        let rate = f64::from(pose.emission_rate()) + f64::from(emitter.emission_rate_variation());
        let lifetime = f64::from(pose.lifespan()) + f64::from(emitter.lifespan_variation());
        let estimate = rate * lifetime * STOCK_CAPACITY_HEADROOM;
        self.reserve_capacity_estimate(estimate)
    }

    /// Applies one stock-format estimate without shrinking retained storage.
    fn reserve_capacity_estimate(
        &mut self,
        estimate: f64,
    ) -> Result<(), M2ParticleSimulationError> {
        if !estimate.is_finite() || estimate < 0.0 || estimate > usize::MAX as f64 {
            return Err(M2ParticleSimulationError::Capacity);
        }
        let required = estimate.trunc() as usize;
        if required > self.capacity {
            self.reserve_particle_storage(required)?;
        }
        Ok(())
    }

    /// Grows the fixed-slot storage and shifts every retained address phase if
    /// its base allocation moves. The active list has separate ownership.
    fn reserve_particle_storage(
        &mut self,
        required: usize,
    ) -> Result<(), M2ParticleSimulationError> {
        let old_base_phase = (!self.particles.is_empty())
            .then(|| ((self.particles.as_ptr().addr() >> 5) & 0x7f) as u8);
        self.particles
            .try_reserve_exact(required.saturating_sub(self.particles.len()))
            .map_err(|_| M2ParticleSimulationError::Allocation)?;
        self.active_pool_slots
            .try_reserve_exact(required.saturating_sub(self.active_pool_slots.len()))
            .map_err(|_| M2ParticleSimulationError::Allocation)?;
        if let Some(old_base_phase) = old_base_phase {
            let new_base_phase = ((self.particles.as_ptr().addr() >> 5) & 0x7f) as u8;
            let delta = new_base_phase.wrapping_sub(old_base_phase) & 0x7f;
            for particle in &mut self.particles {
                particle.shift_pool_address_phase(delta);
            }
        }
        self.capacity = required;
        Ok(())
    }
}

/// Ordinary float tracks use step or linear sampling (`0x0082B340`),
/// so their largest stored value bounds every emission-rate/lifespan sample.
fn maximum_authored_value(track: &M2Track<f32>) -> f64 {
    track
        .channels()
        .iter()
        .flat_map(|channel| channel.values())
        .copied()
        .map(f64::from)
        .fold(0.0, f64::max)
}
