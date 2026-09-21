//! Font cache tables retain their backing-allocation charges through worker loans.

use crate::FontError;
use solarity_asset::{AssetError, AssetReadBudget};
use solarity_cpu::{ByteReservation, CpuError, CpuStorageKind};
use std::{collections::hash_map::RandomState, hash::Hash, ops::Deref};

type Map<K, V> = hashbrown::HashMap<K, V, RandomState>;

pub(super) struct CacheMap<K, V> {
    values: Map<K, V>,
    memory: Option<ByteReservation>,
    policy: Option<AssetReadBudget>,
}
impl<K, V> Default for CacheMap<K, V> {
    fn default() -> Self {
        Self {
            values: Map::with_hasher(RandomState::new()),
            memory: None,
            policy: None,
        }
    }
}
impl<K, V> Deref for CacheMap<K, V> {
    type Target = Map<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<K: Eq + Hash, V> CacheMap<K, V> {
    pub(super) fn policy(&self) -> Option<&AssetReadBudget> {
        self.policy.as_ref()
    }

    pub(super) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.values.get_mut(key)
    }
    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.values.values_mut()
    }
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(super) fn retain(&mut self, keep: impl FnMut(&K, &mut V) -> bool) {
        self.values.retain(keep);
        if self.values.is_empty() {
            self.clear();
        }
    }
    pub(super) fn insert(
        &mut self,
        budget: Option<&AssetReadBudget>,
        key: K,
        value: V,
    ) -> Result<(), FontError> {
        let additional = usize::from(!self.values.contains_key(&key));
        let capacity = self
            .values
            .len()
            .checked_add(additional)
            .ok_or(CpuError::StorageSizeOverflow)
            .map_err(AssetError::from)?;
        self.reserve(budget, capacity)?;
        self.values.insert(key, value);
        Ok(())
    }
    pub(super) fn reserve(
        &mut self,
        budget: Option<&AssetReadBudget>,
        capacity: usize,
    ) -> Result<(), FontError> {
        let policy = budget.cloned().or_else(|| self.policy.clone());
        if let Some(policy) = &policy {
            match &mut self.memory {
                Some(memory) => memory
                    .transfer(policy.storage(), policy.class(), CpuStorageKind::Result)
                    .map_err(AssetError::from)?,
                None if self.values.allocation_size() != 0 => {
                    self.memory = Some(
                        policy
                            .storage()
                            .reserve(
                                policy.class(),
                                CpuStorageKind::Result,
                                self.values.allocation_size(),
                            )
                            .map_err(AssetError::from)?,
                    )
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
                        CpuStorageKind::Result,
                        table_bound::<(K, V)>(capacity)?,
                    )
                })
                .transpose()
                .map_err(AssetError::from)?;
            let mut replacement = Map::with_hasher(self.values.hasher().clone());
            replacement
                .try_reserve(capacity)
                .map_err(|_| AssetError::from(CpuError::StorageAllocation))?;
            if let Some(memory) = &mut memory {
                memory
                    .resize(replacement.allocation_size())
                    .map_err(AssetError::from)?;
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

/// Pixel pages and their append journals reserve before growing and retain charges.
pub(crate) struct FontBuffer<T> {
    values: Vec<T>,
    memory: Option<ByteReservation>,
    policy: Option<AssetReadBudget>,
}
impl<T> Default for FontBuffer<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            memory: None,
            policy: None,
        }
    }
}
impl<T> FontBuffer<T> {
    pub(super) fn policy(&self) -> Option<&AssetReadBudget> {
        self.policy.as_ref()
    }
    pub(super) fn reserve(
        &mut self,
        budget: Option<&AssetReadBudget>,
        capacity: usize,
    ) -> Result<(), FontError> {
        if capacity <= self.values.capacity() {
            return Ok(());
        }
        let policy = budget.cloned().or_else(|| self.policy.clone());
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
            .map_err(AssetError::from)?;
        let mut memory = policy
            .as_ref()
            .map(|policy| {
                policy
                    .storage()
                    .reserve(policy.class(), CpuStorageKind::Result, bytes)
            })
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
    pub(super) fn reserve_one(
        &mut self,
        budget: Option<&AssetReadBudget>,
    ) -> Result<(), FontError> {
        if self.values.len() == self.values.capacity() {
            let capacity = self
                .values
                .len()
                .checked_add(1)
                .and_then(usize::checked_next_power_of_two)
                .ok_or(CpuError::StorageSizeOverflow)
                .map_err(AssetError::from)?;
            self.reserve(budget, capacity)?;
        }
        Ok(())
    }
    pub(super) fn push(
        &mut self,
        budget: Option<&AssetReadBudget>,
        value: T,
    ) -> Result<(), FontError> {
        self.reserve_one(budget)?;
        self.values.push(value);
        Ok(())
    }
}
impl FontBuffer<u8> {
    pub(super) fn zeroed(
        budget: Option<&AssetReadBudget>,
        bytes: usize,
    ) -> Result<Self, FontError> {
        let mut buffer = Self::default();
        buffer.reserve(budget, bytes)?;
        buffer.values.resize(bytes, 0);
        Ok(buffer)
    }
}
impl<T> Deref for FontBuffer<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<T> std::ops::DerefMut for FontBuffer<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
impl<T: PartialEq> PartialEq for FontBuffer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for FontBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(f)
    }
}

impl<'a, T> IntoIterator for &'a FontBuffer<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

#[cfg(test)]
#[path = "../../tests/font/storage.rs"]
mod tests;

impl<K: Eq + Hash, V: PartialEq> PartialEq for CacheMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for CacheMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(f)
    }
}
