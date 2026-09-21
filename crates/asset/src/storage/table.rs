//! Typed domain tables retain their actual allocation admission through ownership transfers.

use crate::{AssetError, AssetReadBudget};
use solarity_cpu::{ByteReservation, CpuError, CpuStorageKind};
use std::{collections::hash_map::RandomState, hash::Hash, ops::Deref};

type Map<K, V> = hashbrown::HashMap<K, V, RandomState>;

/// A typed hash table with explicit byte admission and no implicit mutable growth.
pub struct AssetStorageMap<K, V> {
    values: Map<K, V>,
    memory: Option<ByteReservation>,
    policy: Option<AssetReadBudget>,
    kind: CpuStorageKind,
}
impl<K, V> Default for AssetStorageMap<K, V> {
    fn default() -> Self {
        Self {
            values: Map::with_hasher(RandomState::new()),
            memory: None,
            policy: None,
            kind: CpuStorageKind::Result,
        }
    }
}
impl<K, V> Deref for AssetStorageMap<K, V> {
    type Target = Map<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<K: Eq + Hash, V> AssetStorageMap<K, V> {
    /// Creates an empty metadata table; no allocation happens before reserve/insert.
    pub fn metadata() -> Self {
        Self {
            kind: CpuStorageKind::Metadata,
            ..Self::default()
        }
    }
    /// Removes a key and value without reallocating the retained table.
    pub fn remove_entry(&mut self, key: &K) -> Option<(K, V)> {
        self.values.remove_entry(key)
    }
    /// Publishes into capacity admitted before a multi-container metadata transition.
    pub(crate) fn insert_reserved(&mut self, key: K, value: V) {
        assert!(self.values.contains_key(&key) || self.values.len() < self.values.capacity());
        self.values.insert(key, value);
    }
    /// Removes an entry while retaining admitted table capacity for reuse.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.values.remove(key)
    }
    /// Returns the storage policy already attached to this allocation.
    pub fn policy(&self) -> Option<&AssetReadBudget> {
        self.policy.as_ref()
    }

    /// Borrows a retained value without permitting table growth.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.values.get_mut(key)
    }
    /// Borrows values without permitting table growth.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.values.values_mut()
    }
    /// Releases values and their backing allocation before dropping admission.
    pub fn clear(&mut self) {
        *self = Self {
            kind: self.kind,
            ..Self::default()
        };
    }
    /// Removes unused values, releasing storage when no entries remain.
    pub fn retain(&mut self, keep: impl FnMut(&K, &mut V) -> bool) {
        self.values.retain(keep);
        if self.values.is_empty() {
            self.clear();
        }
    }
    /// Admits table growth before inserting or replacing a value.
    /// # Errors
    /// Returns byte pressure, size overflow or allocation failure.
    pub fn insert(
        &mut self,
        budget: Option<&AssetReadBudget>,
        key: K,
        value: V,
    ) -> Result<(), AssetError> {
        let additional = usize::from(!self.values.contains_key(&key));
        let capacity = self
            .values
            .len()
            .checked_add(additional)
            .ok_or(CpuError::StorageSizeOverflow)?;
        self.reserve(budget, capacity)?;
        self.values.insert(key, value);
        Ok(())
    }
    /// Admits a complete replacement while the old table remains charged.
    /// # Errors
    /// Refusal preserves entries and their existing allocation.
    pub fn reserve(
        &mut self,
        budget: Option<&AssetReadBudget>,
        capacity: usize,
    ) -> Result<(), AssetError> {
        let policy = budget.cloned().or_else(|| self.policy.clone());
        if let Some(policy) = &policy {
            match &mut self.memory {
                Some(memory) => memory.transfer(policy.storage(), policy.class(), self.kind)?,
                None if self.values.allocation_size() != 0 => {
                    self.memory = Some(policy.storage().reserve(
                        policy.class(),
                        self.kind,
                        self.values.allocation_size(),
                    )?)
                }
                None => (),
            }
        }
        self.policy = policy;
        if capacity > self.values.capacity() {
            // Admit a complete replacement while the old allocation remains charged.
            let mut memory = self
                .policy
                .as_ref()
                .map(|budget| {
                    budget.storage().reserve(
                        budget.class(),
                        self.kind,
                        table_bound::<(K, V)>(capacity)?,
                    )
                })
                .transpose()?;
            let mut replacement = Map::with_hasher(self.values.hasher().clone());
            replacement
                .try_reserve(capacity)
                .map_err(|_| AssetError::from(CpuError::StorageAllocation))?;
            if let Some(memory) = &mut memory {
                memory.resize(replacement.allocation_size())?;
            }
            replacement.extend(self.values.drain());
            let old = std::mem::replace(&mut self.values, replacement);
            drop(old);
            self.memory = memory;
        }
        Ok(())
    }
}

/// Bound for the pinned hashbrown 0.17.1 bucket/ctrl layout. Sixteen-byte groups
/// dominate all supported backends; small elements can select sixteen buckets.
/// The public allocation_size reconciles the exact retained allocation afterward.
fn table_bound<T>(capacity: usize) -> Result<usize, CpuError> {
    let buckets = if capacity < 15 {
        let minimum = match size_of::<T>() {
            0..=1 => 14,
            2..=3 => 7,
            _ => 3,
        };
        (capacity.max(minimum) + 1).next_power_of_two()
    } else {
        capacity
            .checked_mul(8)
            .map(|value| value / 7)
            .and_then(usize::checked_next_power_of_two)
            .ok_or(CpuError::StorageSizeOverflow)?
    };
    let alignment = align_of::<T>().max(16);
    let payload = buckets
        .checked_mul(size_of::<T>())
        .and_then(|value| value.checked_add(alignment - 1))
        .map(|value| value & !(alignment - 1))
        .ok_or(CpuError::StorageSizeOverflow)?;
    payload
        .checked_add(buckets)
        .and_then(|value| value.checked_add(16))
        .ok_or(CpuError::StorageSizeOverflow)
}

impl<K: Eq + Hash, V: PartialEq> PartialEq for AssetStorageMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for AssetStorageMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(f)
    }
}

#[cfg(test)]
#[path = "../../tests/storage/table.rs"]
mod tests;
