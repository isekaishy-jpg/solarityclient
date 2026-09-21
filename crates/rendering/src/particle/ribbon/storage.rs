//! A ribbon's fixed stock history owns its CPU capacity charge.

use super::{M2RibbonSection, M2RibbonTrail};
use solarity_cpu::{
    ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind,
    CpuStorageReservation, CpuStorageWorkingSet,
};

/// Unique charge released only after the associated history allocation.
pub(super) struct RibbonMemory(ByteReservation);

impl std::fmt::Debug for RibbonMemory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RibbonMemory")
            .field(&self.0.bytes())
            .finish()
    }
}

impl M2RibbonTrail {
    /// Binds the already allocated authored ring before its first worker transfer.
    /// Fixed-window insertion cannot grow it during a simulation update.
    ///
    /// # Errors
    /// Reports capacity overflow or budget refusal without changing trail history.
    pub fn reserve_cpu_storage(&mut self, budget: &CpuStorageBudget) -> Result<(), CpuError> {
        let mut working_set = CpuStorageWorkingSet::default();
        self.include_cpu_storage(budget, &mut working_set)?;
        let mut reservation =
            budget.reserve_working_set(CpuStorageClass::Frame, working_set.bytes())?;
        self.reserve_cpu_storage_reserved(&mut reservation)
    }

    /// Adds the fixed history allocation to a connected admission without mutating it.
    /// # Errors
    /// Reports byte overflow before changing the trail or its charge.
    pub fn include_cpu_storage(
        &self,
        budget: &CpuStorageBudget,
        working_set: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        let bytes = self
            .sections
            .capacity()
            .checked_mul(size_of::<M2RibbonSection>())
            .ok_or(CpuError::StorageSizeOverflow)?;
        working_set.include(
            self.memory.as_ref().map_or(bytes, |memory| {
                memory.0.admission_bytes(budget, CpuStorageClass::Frame)
            }),
            0,
        )
    }

    /// Adopts fixed history using capacity protected for the complete model.
    /// # Errors
    /// Refuses an underestimated reservation without changing trail history.
    pub fn reserve_cpu_storage_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
    ) -> Result<(), CpuError> {
        let kind = CpuStorageKind::Scratch;
        if let Some(memory) = &mut self.memory {
            memory.0.transfer_reserved(reservation, kind)?;
        } else {
            let bytes = self
                .sections
                .capacity()
                .checked_mul(size_of::<M2RibbonSection>())
                .ok_or(CpuError::StorageSizeOverflow)?;
            self.memory = Some(RibbonMemory(reservation.reserve(kind, bytes)?));
        }
        Ok(())
    }

    /// Actual retained history capacity, independent of the number of live edges.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.sections.capacity() * size_of::<M2RibbonSection>()
    }
}
