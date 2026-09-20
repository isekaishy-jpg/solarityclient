//! Standard containers with capacity charges and peak-safe replacement growth.

use super::{ByteReservation, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use crate::CpuError;
use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut, RangeFull},
};

/// Counts logical element capacity, including zero-sized element storage correctly.
fn bytes<T>(capacity: usize) -> Result<usize, CpuError> {
    capacity
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(CpuError::StorageSizeOverflow)
}

/// Metadata values remain accessible as a slice, never as an unbudgeted growing Vec.
pub(crate) struct StorageVec<T> {
    values: Vec<T>,
    memory: Option<ByteReservation>,
}
impl<T> Default for StorageVec<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            memory: None,
        }
    }
}
impl<T> StorageVec<T> {
    /// Empty registration owns no allocation and consumes no byte allowance.
    pub(crate) const fn new() -> Self {
        Self {
            values: Vec::new(),
            memory: None,
        }
    }

    /// Growth reserves a second allocation while the old one remains charged.
    /// Failed allocation/reconciliation preserves values and their original storage.
    pub(crate) fn reserve(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        capacity: usize,
    ) -> Result<(), CpuError> {
        if let Some(memory) = &mut self.memory {
            memory.transfer(budget, class, kind)?;
        }
        if capacity <= self.values.capacity() {
            return Ok(());
        }
        let mut memory = budget.reserve(class, kind, bytes::<T>(capacity)?)?;
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(capacity)
            .map_err(|_| CpuError::StorageAllocation)?;
        memory.resize(bytes::<T>(replacement.capacity())?)?;
        replacement.append(&mut self.values);
        let old = std::mem::replace(&mut self.values, replacement);
        drop(old);
        self.memory = Some(memory);
        Ok(())
    }
    /// Borrows a fixed-capacity writer without exposing the growable Vec.
    pub(crate) fn writer(&mut self) -> super::FixedWriter<'_, T> {
        super::FixedWriter::new(&mut self.values)
    }
    pub(crate) fn capacity(&self) -> usize {
        self.values.capacity()
    }
    pub(crate) fn allocation_id(&self) -> Option<u64> {
        self.memory.as_ref().map(ByteReservation::allocation_id)
    }
    /// Internal producers have already admitted their complete node/edge bound.
    pub(crate) fn push(&mut self, value: T) {
        assert!(
            self.values.len() < self.values.capacity(),
            "admitted metadata must fit reserved capacity"
        );
        self.values.push(value);
    }
    /// Metadata reset retains its allocation and accounting until explicit drop/growth.
    pub(crate) fn clear(&mut self) {
        self.values.clear();
    }
    pub(crate) fn drain(&mut self, range: RangeFull) -> std::vec::Drain<'_, T> {
        self.values.drain(range)
    }
    /// Retains existing elements without changing allocation ownership.
    pub(crate) fn retain(&mut self, predicate: impl FnMut(&T) -> bool) {
        self.values.retain(predicate);
    }
    /// Initializes only slots inside the already admitted capacity.
    pub(crate) fn resize_with(&mut self, len: usize, create: impl FnMut() -> T) {
        assert!(
            len <= self.values.capacity(),
            "admitted slots must fit reserved capacity"
        );
        self.values.resize_with(len, create);
    }
    /// Clones only elements, into already reserved storage; it cannot grow the buffer.
    pub(crate) fn extend_from_slice(&mut self, values: &[T])
    where
        T: Clone,
    {
        assert!(
            values.len() <= self.values.capacity() - self.values.len(),
            "admitted metadata must fit reserved capacity"
        );
        self.values.extend_from_slice(values);
    }
}
impl<T> Deref for StorageVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}
impl<T> DerefMut for StorageVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}

/// Ready/propagation rings use the same accounting and retain their FIFO semantics.
pub(crate) struct StorageDeque<T> {
    values: VecDeque<T>,
    memory: Option<ByteReservation>,
}
impl<T> Default for StorageDeque<T> {
    fn default() -> Self {
        Self {
            values: VecDeque::new(),
            memory: None,
        }
    }
}
impl<T> StorageDeque<T> {
    /// Holds both allocation charges until a replacement can own all queued values.
    pub(crate) fn reserve(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        capacity: usize,
    ) -> Result<(), CpuError> {
        if let Some(memory) = &mut self.memory {
            memory.transfer(budget, class, kind)?;
        }
        if capacity <= self.values.capacity() {
            return Ok(());
        }
        let mut memory = budget.reserve(class, kind, bytes::<T>(capacity)?)?;
        let mut replacement = VecDeque::new();
        replacement
            .try_reserve_exact(capacity)
            .map_err(|_| CpuError::StorageAllocation)?;
        memory.resize(bytes::<T>(replacement.capacity())?)?;
        replacement.extend(self.values.drain(..));
        let old = std::mem::replace(&mut self.values, replacement);
        drop(old);
        self.memory = Some(memory);
        Ok(())
    }
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub(crate) fn pop_front(&mut self) -> Option<T> {
        self.values.pop_front()
    }
    /// Selects eligible cold service work without rotating skipped FIFO records.
    pub(crate) fn pop_matching(&mut self, predicate: impl FnMut(&T) -> bool) -> Option<T> {
        let index = self.values.iter().position(predicate)?;
        self.values.remove(index)
    }
    /// Every producer reserves queue/phase maxima before enqueueing a value.
    pub(crate) fn push_back(&mut self, value: T) {
        assert!(
            self.values.len() < self.values.capacity(),
            "admitted queue entries must fit reserved capacity"
        );
        self.values.push_back(value);
    }
    pub(crate) fn clear(&mut self) {
        self.values.clear();
    }
}
