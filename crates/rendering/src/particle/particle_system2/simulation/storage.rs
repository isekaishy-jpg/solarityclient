//! Physical worker storage is admitted independently of the stock live-pool limit.

use super::{M2ParticleSimulation, M2ParticleSimulationError};
use crate::particle::particle_system2::M2ParticleState;
use solarity_cpu::{
    ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind,
    CpuStorageReservation, CpuStorageWorkingSet,
};

/// Unique capacity charge retained after a worker returns the simulation.
pub(super) struct ParticleMemory(ByteReservation);

impl std::fmt::Debug for ParticleMemory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("ParticleMemory")
            .field(&self.0.bytes())
            .finish()
    }
}

/// Arithmetic includes both active and free slot lists, not just live particles.
fn bytes(particles: usize, active: usize, free: usize) -> Result<usize, CpuError> {
    active
        .checked_add(free)
        .and_then(|slots| slots.checked_mul(size_of::<usize>()))
        .and_then(|slots| {
            particles
                .checked_mul(size_of::<M2ParticleState>())
                .and_then(|particles| slots.checked_add(particles))
        })
        .ok_or(CpuError::StorageSizeOverflow)
}

/// Complete replacement storage is charged while the previous allocation is live.
fn copy_capacity<T: Copy>(values: &[T], capacity: usize) -> Result<Vec<T>, CpuError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| CpuError::StorageAllocation)?;
    result.extend_from_slice(values);
    Ok(result)
}

impl M2ParticleSimulation {
    /// Admits physical storage before dispatch without changing stock's live capacity,
    /// emission remainder, random stream or active/free slot identities. Address-derived
    /// twinkle phases follow relocation just as in native particle-pool growth.
    ///
    /// # Errors
    /// Reports byte overflow, budget refusal or allocation failure. Existing simulation
    /// values remain unchanged on refusal; an existing unbound allocation may be adopted.
    pub fn reserve_cpu_storage(
        &mut self,
        budget: &CpuStorageBudget,
        maximum: usize,
    ) -> Result<(), CpuError> {
        let mut working_set = CpuStorageWorkingSet::default();
        self.include_cpu_storage(budget, maximum, &mut working_set)?;
        let mut reservation =
            budget.reserve_working_set(CpuStorageClass::Frame, working_set.bytes())?;
        self.reserve_cpu_storage_reserved(&mut reservation, maximum)
    }

    /// Adds adoption and replacement demand in allocation order without changing state.
    /// # Errors
    /// Reports checked capacity overflow before any allocation or simulation changes.
    pub fn include_cpu_storage(
        &self,
        budget: &CpuStorageBudget,
        maximum: usize,
        working_set: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        let maximum = maximum.max(self.capacity);
        let retained = bytes(
            self.particles.capacity(),
            self.active_pool_slots.capacity(),
            self.free_pool_slots.capacity(),
        )?;
        let adoption = self.memory.as_ref().map_or(retained, |memory| {
            memory.0.admission_bytes(budget, CpuStorageClass::Frame)
        });
        let (replacement, retired) = if maximum <= self.particles.capacity()
            && maximum <= self.active_pool_slots.capacity()
            && maximum <= self.free_pool_slots.capacity()
        {
            (0, 0)
        } else {
            (
                bytes(
                    maximum.max(self.particles.capacity()),
                    maximum.max(self.active_pool_slots.capacity()),
                    maximum.max(self.free_pool_slots.capacity()),
                )?,
                retained,
            )
        };
        working_set.include(
            adoption
                .checked_add(replacement)
                .ok_or(CpuError::StorageSizeOverflow)?,
            retired,
        )
    }

    /// Funds the retained arrays from a previously admitted connected working set.
    /// # Errors
    /// Reports insufficient reserved capacity or allocation failure before replacing values.
    pub fn reserve_cpu_storage_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
        maximum: usize,
    ) -> Result<(), CpuError> {
        let kind = CpuStorageKind::Scratch;
        if let Some(memory) = &mut self.memory {
            memory.0.transfer_reserved(reservation, kind)?;
        } else {
            self.memory = Some(ParticleMemory(reservation.reserve(
                kind,
                bytes(
                    self.particles.capacity(),
                    self.active_pool_slots.capacity(),
                    self.free_pool_slots.capacity(),
                )?,
            )?));
        }
        let maximum = maximum.max(self.capacity);
        if maximum <= self.particles.capacity()
            && maximum <= self.active_pool_slots.capacity()
            && maximum <= self.free_pool_slots.capacity()
        {
            return Ok(());
        }
        let capacities = [
            maximum.max(self.particles.capacity()),
            maximum.max(self.active_pool_slots.capacity()),
            maximum.max(self.free_pool_slots.capacity()),
        ];
        let mut memory =
            reservation.reserve(kind, bytes(capacities[0], capacities[1], capacities[2])?)?;
        let mut particles = copy_capacity(&self.particles, capacities[0])?;
        let active = copy_capacity(&self.active_pool_slots, capacities[1])?;
        let free = copy_capacity(&self.free_pool_slots, capacities[2])?;
        memory.resize_reserved(
            reservation,
            bytes(particles.capacity(), active.capacity(), free.capacity())?,
        )?;
        if !particles.is_empty() {
            let old_phase = ((self.particles.as_ptr().addr() >> 5) & 0x7f) as u8;
            let new_phase = ((particles.as_ptr().addr() >> 5) & 0x7f) as u8;
            for particle in &mut particles {
                particle.shift_pool_address_phase(new_phase.wrapping_sub(old_phase) & 0x7f);
            }
        }
        self.particles = particles;
        self.active_pool_slots = active;
        self.free_pool_slots = free;
        if let Some(retired) = self.memory.replace(ParticleMemory(memory)) {
            reservation.recycle(retired.0)?;
        }
        Ok(())
    }

    /// Reports retained capacity, including the reusable free-slot list.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.particles.capacity() * size_of::<M2ParticleState>()
            + (self.active_pool_slots.capacity() + self.free_pool_slots.capacity())
                * size_of::<usize>()
    }

    /// Every list must fit before a bound simulator changes its stock live limit.
    pub(super) fn check_cpu_storage(
        &self,
        requested: usize,
    ) -> Result<(), M2ParticleSimulationError> {
        let available = self.physical_capacity();
        if self.memory.is_some() && requested > available {
            return Err(M2ParticleSimulationError::StorageCapacity {
                requested,
                available,
            });
        }
        Ok(())
    }

    /// A pool slot may appear in either active or free storage after any update.
    fn physical_capacity(&self) -> usize {
        self.particles
            .capacity()
            .min(self.active_pool_slots.capacity())
            .min(self.free_pool_slots.capacity())
    }
}
