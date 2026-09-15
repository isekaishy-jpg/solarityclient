//! Typed immutable result leases share one allocation and one capacity charge.

use super::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, StorageVec};
use crate::CpuError;
use std::{ops::Deref, sync::Arc};

/// A producer owns writable contiguous output. Nested allocations in T require
/// their own reservations; this page accounts for element capacity only.
pub struct CpuResultPage<T> {
    values: StorageVec<T>,
    budget: CpuStorageBudget,
    class: CpuStorageClass,
}
impl<T> CpuResultPage<T> {
    /// Reserves output capacity before the producer transfers its mandatory inputs.
    /// # Errors
    /// Reports byte admission, size overflow or allocation failure.
    pub fn new(
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        capacity: usize,
    ) -> Result<Self, CpuError> {
        let mut page = Self {
            values: StorageVec::default(),
            budget: budget.clone(),
            class,
        };
        page.reserve(capacity)?;
        Ok(page)
    }
    /// Explicit growth includes the old and new allocations in peak admission.
    /// # Errors
    /// Rejection leaves all existing values intact.
    pub fn reserve(&mut self, capacity: usize) -> Result<(), CpuError> {
        self.values
            .reserve(&self.budget, self.class, CpuStorageKind::Result, capacity)
    }
    /// Appends without allocation.
    /// # Errors
    /// A full page returns the value to its caller.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        if self.values.len() == self.values.capacity() {
            return Err(value);
        }
        self.values.push(value);
        Ok(())
    }
    /// Keeps warm output storage charged while removing its current values.
    pub fn clear(&mut self) {
        self.values.clear();
    }
    /// Reclassifies an exclusively owned allocation without copying its contents.
    /// # Errors
    /// Destination saturation preserves the original owner and accounting.
    pub fn transfer(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<(), CpuError> {
        self.values.reserve(
            budget,
            class,
            CpuStorageKind::Result,
            self.values.capacity(),
        )?;
        self.budget = budget.clone();
        self.class = class;
        Ok(())
    }
    /// Publishes immutable output. Consumers pin the same bytes and reservation.
    #[must_use]
    pub fn freeze(self) -> CpuResultLease<T> {
        CpuResultLease {
            page: Arc::new(self),
        }
    }
    /// Identity of the underlying capacity, shared by every consumer lease.
    #[must_use]
    pub fn allocation_id(&self) -> Option<u64> {
        self.values.allocation_id()
    }
}
impl<T> Deref for CpuResultPage<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}

/// Last-consumer disposal frees the page and releases its capacity charge.
pub struct CpuResultLease<T> {
    page: Arc<CpuResultPage<T>>,
}
impl<T> Clone for CpuResultLease<T> {
    fn clone(&self) -> Self {
        Self {
            page: Arc::clone(&self.page),
        }
    }
}
impl<T> Deref for CpuResultLease<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.page
    }
}
impl<T> CpuResultLease<T> {
    /// Identity of the underlying capacity, shared by every consumer lease.
    #[must_use]
    pub fn allocation_id(&self) -> Option<u64> {
        self.page.allocation_id()
    }
    /// Reclaims writable storage only when no other consumer still pins it.
    /// # Errors
    /// Other live consumers return the original lease without losing ownership.
    pub fn try_reclaim(self) -> Result<CpuResultPage<T>, Self> {
        Arc::try_unwrap(self.page).map_err(|page| Self { page })
    }
}
