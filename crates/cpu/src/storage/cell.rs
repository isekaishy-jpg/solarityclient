//! Exclusive job ownership moves a small handle while its admitted value stays in place.

use super::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, StorageVec};
use crate::CpuError;

/// One initialized value and its unique capacity charge. Nested allocations
/// remain the domain's responsibility, as with other CPU result storage.
pub struct CpuOwnedCell<T> {
    value: StorageVec<T>,
}

impl<T> CpuOwnedCell<T> {
    /// Admits storage before invoking the initializer. Callers can keep pending
    /// inputs borrowed until that invocation, which follows successful reservation.
    ///
    /// # Errors
    /// Capacity, arithmetic or allocation failure leaves the initializer uncalled.
    pub fn new_with(
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        initialize: impl FnOnce() -> T,
    ) -> Result<Self, CpuError> {
        let mut value = StorageVec::new();
        value.reserve(budget, class, kind, 1)?;
        value.push(initialize());
        Ok(Self { value })
    }

    /// Reassigns retained storage without relocating the value or double charging it.
    ///
    /// # Errors
    /// Rejected destination admission preserves the original charge and value.
    pub fn transfer(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
    ) -> Result<(), CpuError> {
        self.value.reserve(budget, class, kind, 1)
    }

    /// Borrows the exclusively owned value without moving it out of admitted storage.
    pub fn value(&self) -> &T {
        &self.value[0]
    }

    /// Mutates initialized state; neither the cell nor its capacity can be replaced.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value[0]
    }
}
