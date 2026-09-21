//! A ribbon's fixed stock history owns its CPU capacity charge.

use super::{M2RibbonSection, M2RibbonTrail};
use solarity_cpu::{ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};

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
        let class = CpuStorageClass::Frame;
        let kind = CpuStorageKind::Scratch;
        if let Some(memory) = &mut self.memory {
            memory.0.transfer(budget, class, kind)?;
        } else {
            let bytes = self
                .sections
                .capacity()
                .checked_mul(size_of::<M2RibbonSection>())
                .ok_or(CpuError::StorageSizeOverflow)?;
            self.memory = Some(RibbonMemory(budget.reserve(class, kind, bytes)?));
        }
        Ok(())
    }

    /// Actual retained history capacity, independent of the number of live edges.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.sections.capacity() * size_of::<M2RibbonSection>()
    }
}
