//! Capacity-owning domain buffers and nonallocating output writers.

use super::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuStorageReservation, StorageVec};
use crate::CpuError;
use std::ops::{Deref, DerefMut};

/// Domain storage retains its byte charge across job transfer, clearing and reuse.
/// Nested allocations in T remain separately owned and accounted by the domain.
pub struct CpuBuffer<T> {
    values: StorageVec<T>,
}
impl<T> Default for CpuBuffer<T> {
    fn default() -> Self {
        Self {
            values: StorageVec::new(),
        }
    }
}
impl<T> CpuBuffer<T> {
    /// Admits full capacity before the producer relinquishes required inputs.
    /// # Errors
    /// Byte pressure, size overflow and allocation failure preserve existing values.
    pub fn reserve(
        &mut self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        capacity: usize,
    ) -> Result<(), CpuError> {
        self.values.reserve(budget, class, kind, capacity)
    }
    /// Plans additional headroom for retained storage adoption and peak-safe growth.
    /// # Errors
    /// Returns size overflow without changing values, capacity or admission.
    pub fn reservation_bytes(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        capacity: usize,
    ) -> Result<usize, CpuError> {
        self.values.reservation_bytes(budget, class, capacity)
    }

    /// Capacity returned to the working set when this replacement frees its old buffer.
    #[must_use]
    pub fn replacement_credit(&self, capacity: usize) -> usize {
        self.values.replacement_credit(capacity)
    }

    /// Grows from a connected working set admitted before any producer starts.
    /// # Errors
    /// Returns insufficient reserved capacity, size overflow or allocation failure.
    pub fn reserve_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
        kind: CpuStorageKind,
        capacity: usize,
    ) -> Result<(), CpuError> {
        self.values.reserve_reserved(reservation, kind, capacity)
    }

    /// Opens a writer that can use existing capacity but cannot allocate more.
    pub fn writer(&mut self) -> FixedWriter<'_, T> {
        self.values.writer()
    }
    /// Removes values while preserving their retained capacity charge.
    pub fn clear(&mut self) {
        self.values.clear();
    }
    /// Moves values out in publication order without giving away the allocation.
    pub fn drain(&mut self) -> impl ExactSizeIterator<Item = T> + '_ {
        self.values.drain(..)
    }
    /// Returns the last value while retaining its admitted backing storage.
    pub fn pop(&mut self) -> Option<T> {
        self.values.pop()
    }
    /// Drops a suffix without changing retained allocation ownership.
    pub fn truncate(&mut self, length: usize) {
        self.values.writer().truncate(length);
    }
    /// Appends one value to admitted storage.
    /// # Errors
    /// Exhausted capacity leaves all existing output unchanged.
    pub fn push(&mut self, value: T) -> Result<(), CpuError> {
        self.writer().push(value)
    }
    /// Copies values into previously admitted storage.
    /// # Errors
    /// Exhausted capacity leaves all existing output unchanged.
    pub fn extend_from_slice(&mut self, values: &[T]) -> Result<(), CpuError>
    where
        T: Clone,
    {
        self.writer().extend_from_slice(values)
    }
    /// Returns charged element capacity, independent of current output length.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }
}
impl<T> Deref for CpuBuffer<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}

impl<T> DerefMut for CpuBuffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}

/// A borrowed output boundary never exposes Vec's implicit growth operations.
pub struct FixedWriter<'a, T> {
    values: &'a mut Vec<T>,
}
impl<'a, T> FixedWriter<'a, T> {
    /// Borrows already reserved ordinary storage; no allocation occurs here.
    pub fn new(values: &'a mut Vec<T>) -> Self {
        Self { values }
    }
    /// Verifies an append fits the existing allocation before generating output.
    /// # Errors
    /// Reports the requested and remaining element counts without mutation.
    pub fn require(&self, additional: usize) -> Result<(), CpuError> {
        let available = self.values.capacity() - self.values.len();
        if additional > available {
            return Err(CpuError::OutputCapacity {
                requested: additional,
                available,
            });
        }
        Ok(())
    }
    /// Appends without implicit allocation.
    /// # Errors
    /// Capacity exhaustion leaves the existing output unchanged.
    pub fn push(&mut self, value: T) -> Result<(), CpuError> {
        self.require(1)?;
        self.values.push(value);
        Ok(())
    }
    /// Copies into already admitted output storage.
    /// # Errors
    /// Capacity exhaustion leaves the existing output unchanged.
    pub fn extend_from_slice(&mut self, values: &[T]) -> Result<(), CpuError>
    where
        T: Clone,
    {
        self.require(values.len())?;
        self.values.extend_from_slice(values);
        Ok(())
    }
    /// Restores a prior output length after a failed producer operation.
    pub fn truncate(&mut self, length: usize) {
        self.values.truncate(length);
    }
    /// Keeps allocation ownership while resetting a scratch list.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}
impl<T> Deref for FixedWriter<'_, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.values
    }
}
impl<T> DerefMut for FixedWriter<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.values
    }
}

/// Shared append contract for owned mesh builders and admitted frame writers.
/// Ordinary builders may reserve; FixedWriter can only validate existing capacity.
pub trait OutputBuffer<T>: AsRef<[T]> + AsMut<[T]> {
    /// Checks or reserves the next append range without changing its length.
    /// # Errors
    /// Reports capacity or allocation failure before producing elements.
    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), CpuError>;
    /// Appends one element after capacity admission.
    /// # Errors
    /// Returns a capacity error without altering existing elements.
    fn push(&mut self, value: T) -> Result<(), CpuError>;
    /// Copies an admitted element range.
    /// # Errors
    /// Returns a capacity error without altering existing elements.
    fn extend_from_slice(&mut self, values: &[T]) -> Result<(), CpuError>
    where
        T: Clone;
    /// Rolls output back to an earlier length.
    fn truncate(&mut self, length: usize);
    /// Removes values while retaining the allocation.
    fn clear(&mut self) {
        self.truncate(0);
    }
    /// Current number of initialized elements.
    fn len(&self) -> usize {
        self.as_ref().len()
    }
    /// Whether no output has been initialized.
    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }
}
impl<T> AsRef<[T]> for FixedWriter<'_, T> {
    fn as_ref(&self) -> &[T] {
        self.values
    }
}
impl<T> AsMut<[T]> for FixedWriter<'_, T> {
    fn as_mut(&mut self) -> &mut [T] {
        self.values
    }
}
impl<T> OutputBuffer<T> for FixedWriter<'_, T> {
    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), CpuError> {
        self.require(additional)
    }
    fn push(&mut self, value: T) -> Result<(), CpuError> {
        FixedWriter::push(self, value)
    }
    fn extend_from_slice(&mut self, values: &[T]) -> Result<(), CpuError>
    where
        T: Clone,
    {
        FixedWriter::extend_from_slice(self, values)
    }
    fn truncate(&mut self, length: usize) {
        FixedWriter::truncate(self, length);
    }
}
impl<T> OutputBuffer<T> for Vec<T> {
    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), CpuError> {
        Vec::try_reserve_exact(self, additional).map_err(|_| CpuError::StorageAllocation)
    }
    fn push(&mut self, value: T) -> Result<(), CpuError> {
        FixedWriter::new(self).push(value)
    }
    fn extend_from_slice(&mut self, values: &[T]) -> Result<(), CpuError>
    where
        T: Clone,
    {
        FixedWriter::new(self).extend_from_slice(values)
    }
    fn truncate(&mut self, length: usize) {
        Vec::truncate(self, length);
    }
}
