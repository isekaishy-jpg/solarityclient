//! Typed temporary values cannot escape the synchronous scratch loan.

use super::{
    CpuBuffer, CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuStorageReservation,
    FixedWriter,
};
use crate::CpuError;

/// A domain declares and admits its typed scratch before relinquishing inputs.
/// Results use separate output ownership; scratch has no freeze or drain API.
pub struct CpuScratch<T> {
    values: CpuBuffer<T>,
}

impl<T> Default for CpuScratch<T> {
    fn default() -> Self {
        Self {
            values: CpuBuffer::default(),
        }
    }
}

impl<T> CpuScratch<T> {
    /// Explicit growth is an admission operation, never implicit kernel allocation.
    /// # Errors
    /// Reports byte saturation, overflow or allocation failure without losing storage.
    pub fn reserve(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        capacity: usize,
    ) -> Result<(), CpuError> {
        self.values
            .reserve(budget, class, CpuStorageKind::Scratch, capacity)
    }

    /// Plans this scratch allocation as part of a connected working set.
    /// # Errors
    /// Reports size overflow without changing retained capacity.
    pub fn reservation_bytes(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        capacity: usize,
    ) -> Result<usize, CpuError> {
        self.values.reservation_bytes(budget, class, capacity)
    }

    /// Capacity returned after replacement storage takes over the old scratch allocation.
    #[must_use]
    pub fn replacement_credit(&self, capacity: usize) -> usize {
        self.values.replacement_credit(capacity)
    }

    /// Consumes capacity protected by the transaction's original admission.
    /// # Errors
    /// Reports insufficient reservation, size overflow or allocation failure.
    pub fn reserve_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
        capacity: usize,
    ) -> Result<(), CpuError> {
        self.values
            .reserve_reserved(reservation, CpuStorageKind::Scratch, capacity)
    }

    /// Retained, charged element capacity independent of temporary live length.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }

    /// Only an executor-provided context opens the scratch loan.
    pub(crate) fn scope(&mut self) -> ScratchScope<'_, T> {
        self.values.clear();
        ScratchScope {
            writer: self.values.writer(),
        }
    }
}

/// The writer cannot grow. Its borrow ends before the owning storage can be
/// reused; even a panicking kernel discards temporary values before publication.
pub struct ScratchScope<'scope, T> {
    writer: FixedWriter<'scope, T>,
}

impl<'scope, T> ScratchScope<'scope, T> {
    /// Uses only the preadmitted capacity for the duration of this loan.
    pub fn writer(&mut self) -> &mut FixedWriter<'scope, T> {
        &mut self.writer
    }
}

impl<T> Drop for ScratchScope<'_, T> {
    fn drop(&mut self) {
        self.writer.clear();
    }
}
