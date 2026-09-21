//! Admitted typed storage shared by source metadata and decoded domain owners.
use crate::{AssetError, AssetReadBudget};
use solarity_cpu::{ByteReservation, CpuError, CpuStorageKind};
use std::ops::Deref;

/// Typed buffers reserve before growing and retain their actual allocation charges.
pub struct AssetStorageVec<T> {
    values: Vec<T>,
    memory: Option<ByteReservation>,
    policy: Option<AssetReadBudget>,
    kind: CpuStorageKind,
}
impl<T> Default for AssetStorageVec<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            memory: None,
            policy: None,
            kind: CpuStorageKind::Result,
        }
    }
}
impl<T> AssetStorageVec<T> {
    /// An empty metadata buffer uses the same replacement admission as result storage.
    pub fn metadata() -> Self {
        Self {
            kind: CpuStorageKind::Metadata,
            ..Self::default()
        }
    }
    /// Removes values without releasing admitted warm capacity.
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.values.retain(keep);
    }
    /// Drains values while the buffer continues to own its backing allocation.
    pub fn drain(&mut self) -> std::vec::Drain<'_, T> {
        self.values.drain(..)
    }
    /// Returns the storage policy attached to this allocation.
    pub fn policy(&self) -> Option<&AssetReadBudget> {
        self.policy.as_ref()
    }
    /// Admits replacement capacity before growing the buffer.
    /// # Errors
    /// Returns byte pressure, size overflow or allocation failure.
    pub fn reserve(
        &mut self,
        budget: Option<&AssetReadBudget>,
        capacity: usize,
    ) -> Result<(), AssetError> {
        let policy = budget.cloned().or_else(|| self.policy.clone());
        if let Some(policy) = &policy {
            match &mut self.memory {
                Some(memory) => memory.transfer(policy.storage(), policy.class(), self.kind)?,
                None => {
                    let bytes = self
                        .values
                        .capacity()
                        .checked_mul(size_of::<T>())
                        .ok_or(CpuError::StorageSizeOverflow)?;
                    if bytes != 0 {
                        self.memory =
                            Some(policy.storage().reserve(policy.class(), self.kind, bytes)?);
                    }
                }
            }
        }
        self.policy = policy.clone();
        if capacity <= self.values.capacity() {
            return Ok(());
        }
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
            .map_err(AssetError::from)?;
        let mut memory = policy
            .as_ref()
            .map(|policy| policy.storage().reserve(policy.class(), self.kind, bytes))
            .transpose()
            .map_err(AssetError::from)?;
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(capacity)
            .map_err(|_| AssetError::from(CpuError::StorageAllocation))?;
        if let Some(memory) = &mut memory {
            memory
                .resize(
                    replacement
                        .capacity()
                        .checked_mul(size_of::<T>())
                        .ok_or(CpuError::StorageSizeOverflow)
                        .map_err(AssetError::from)?,
                )
                .map_err(AssetError::from)?;
        }
        replacement.append(&mut self.values);
        let old = std::mem::replace(&mut self.values, replacement);
        drop(old);
        self.memory = memory;
        self.policy = policy;
        Ok(())
    }
    /// Admits geometric growth when the next append needs capacity.
    /// # Errors
    /// Refusal preserves all values and the old allocation.
    pub fn reserve_one(&mut self, budget: Option<&AssetReadBudget>) -> Result<(), AssetError> {
        if self.values.len() == self.values.capacity() {
            let capacity = self
                .values
                .len()
                .checked_add(1)
                .and_then(usize::checked_next_power_of_two)
                .ok_or(CpuError::StorageSizeOverflow)
                .map_err(AssetError::from)?;
            self.reserve(budget, capacity)?;
        } else {
            self.reserve(budget, self.values.capacity())?;
        }
        Ok(())
    }
    /// Publishes into capacity admitted before a multi-container metadata transition.
    pub(crate) fn push_reserved(&mut self, value: T) {
        assert!(self.values.len() < self.values.capacity());
        self.values.push(value);
    }
    /// Appends after admitting any necessary backing storage.
    /// # Errors
    /// Returns byte pressure, size overflow or allocation failure.
    pub fn push(&mut self, budget: Option<&AssetReadBudget>, value: T) -> Result<(), AssetError> {
        self.reserve_one(budget)?;
        self.values.push(value);
        Ok(())
    }
}
impl AssetStorageVec<u8> {
    /// Admits and initializes a complete zero-filled byte buffer.
    /// # Errors
    /// Returns byte pressure, size overflow or allocation failure.
    pub fn zeroed(budget: Option<&AssetReadBudget>, bytes: usize) -> Result<Self, AssetError> {
        let mut buffer = Self::default();
        buffer.reserve(budget, bytes)?;
        buffer.values.resize(bytes, 0);
        Ok(buffer)
    }
}
impl<T> Deref for AssetStorageVec<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<T> std::ops::DerefMut for AssetStorageVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
impl<T: PartialEq> PartialEq for AssetStorageVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for AssetStorageVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(f)
    }
}

impl<'a, T> IntoIterator for &'a AssetStorageVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
