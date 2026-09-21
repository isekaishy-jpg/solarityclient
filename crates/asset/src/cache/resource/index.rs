//! Single-owner resource index visits release notifications instead of all sources.

use std::{hash::Hash, sync::Arc};

use super::{
    ResourceCacheClock, ResourceLease, ResourceWeak,
    lease::{Pin, shared_metadata},
    releases::{Releases, Ticket},
};
use crate::{AssetError, AssetReadBudget, AssetStorageMap, AssetStorageVec};
use solarity_cpu::{ByteReservation, CpuService, CpuStorageBudget};

/// Cache ownership is distinct from the current external consumer pin generation.
struct Entry<K, T> {
    key: K,
    value: Arc<T>,
    pin: ResourceWeak<T>,
    ticket: Ticket,
}

/// Detached keys and payload ownership leave every index/accounting lock before disposal.
pub(crate) struct RetiredResource<K, T> {
    _entry: Entry<K, T>,
    _index_key: K,
}

/// Keys remain domain-defined; this index supplies no guessed age/eviction policy.
pub(crate) struct ResourceCache<K, T> {
    indices: AssetStorageMap<K, usize>,
    entries: AssetStorageVec<Option<Entry<K, T>>>,
    releases: Arc<Releases>,
    release_memory: Option<Arc<ByteReservation>>,
    budget: Option<AssetReadBudget>,
}

impl<K: Eq + Hash, T> Default for ResourceCache<K, T> {
    fn default() -> Self {
        Self {
            indices: AssetStorageMap::metadata(),
            entries: AssetStorageVec::metadata(),
            releases: Arc::default(),
            release_memory: None,
            budget: None,
        }
    }
}

impl<K: Eq + Hash + Clone, T> ResourceCache<K, T> {
    /// M2 lookup membership supplies qualification; other source domains keep immediate policy.
    pub(crate) fn with_retention(clock: ResourceCacheClock) -> Self {
        Self {
            releases: Arc::new(Releases::timed(clock)),
            ..Self::default()
        }
    }

    /// Binds cache control storage at first configured namespace use.
    /// Existing offline buffers are adopted before this owner may serve new demand.
    pub(crate) fn admit(&mut self, storage: Option<&CpuStorageBudget>) -> Result<(), AssetError> {
        if self.budget.is_some() {
            return Ok(());
        }
        let Some(storage) = storage else {
            return Ok(());
        };
        let budget = AssetReadBudget::for_service(storage.clone(), CpuService::Required);
        let memory = shared_metadata::<Releases>(Some(&budget))?;
        self.indices.reserve(Some(&budget), self.indices.len())?;
        self.entries.reserve(Some(&budget), self.entries.len())?;
        self.releases.admit(&budget)?;
        self.release_memory = memory;
        self.budget = Some(budget);
        Ok(())
    }

    /// Registers a namespace maintenance observer at the first load boundary.
    pub(crate) fn subscribe(
        &self,
        watcher: &super::super::source_storage::RetirementSignal,
    ) -> Result<(), AssetError> {
        self.releases.subscribe(watcher)
    }

    /// Reports a deadline without examining any retained source data.
    pub(crate) fn next_delay_ms(&self) -> Option<u32> {
        self.releases.next_delay_ms()
    }

    /// Closing a cache owner makes its registered maintenance immediately useful.
    pub(crate) fn mark_changed(&self) {
        self.releases.mark_changed();
    }

    /// One time observation is shared by all retirements in a service slice.
    pub(crate) fn collection_time(&self) -> u32 {
        self.releases.collection_time()
    }

    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// A hit retains source identity but never authorizes a live scene instance.
    pub(crate) fn get(&mut self, key: &K) -> Result<Option<ResourceLease<T>>, AssetError> {
        let Some(&index) = self.indices.get(key) else {
            return Ok(None);
        };
        let entry = self.entries[index]
            .as_mut()
            .unwrap_or_else(|| unreachable!("registered keys own their entry"));
        if let Some(lease) = entry.pin.upgrade() {
            return Ok(Some(lease));
        }
        // Pressure must leave the old ticket and grace-period notification intact.
        let memory = shared_metadata::<Pin<T>>(self.budget.as_ref())?;
        entry.ticket = self.releases.renew(entry.ticket)?;
        let lease = ResourceLease::cached(
            Arc::clone(&entry.value),
            &self.releases,
            entry.ticket,
            memory,
            self.release_memory.clone(),
        );
        entry.pin = ResourceLease::downgrade(&lease);
        Ok(Some(lease))
    }

    /// Installs one successfully decoded source. A preexisting generation retains authority.
    pub(crate) fn insert(&mut self, key: K, value: T) -> Result<ResourceLease<T>, AssetError> {
        self.insert_shared(key, Arc::new(value))
    }

    /// Locked cache owners retain an outer pin so rejected values dispose after unlocking.
    pub(crate) fn insert_shared(
        &mut self,
        key: K,
        value: Arc<T>,
    ) -> Result<ResourceLease<T>, AssetError> {
        if let Some(existing) = self.get(&key)? {
            return Ok(existing);
        }
        // Every fallible admission precedes ticket publication; after registration,
        // all index writes use capacity already owned by this cache.
        let memory = shared_metadata::<Pin<T>>(self.budget.as_ref())?;
        self.indices
            .reserve(self.budget.as_ref(), self.indices.len() + 1)?;
        if self.indices.len() == self.entries.len() {
            self.entries.reserve_one(self.budget.as_ref())?;
        }
        let index_key = key.clone();
        let ticket = self.releases.register()?;
        let lease = ResourceLease::cached(
            Arc::clone(&value),
            &self.releases,
            ticket,
            memory,
            self.release_memory.clone(),
        );
        self.indices.insert_reserved(index_key, ticket.index);
        while self.entries.len() <= ticket.index {
            self.entries.push_reserved(None);
        }
        self.entries[ticket.index] = Some(Entry {
            key,
            value,
            pin: ResourceLease::downgrade(&lease),
            ticket,
        });
        Ok(lease)
    }

    /// Collects only notified cache-only entries at the domain's existing boundary.
    /// Payloads and key destructors execute after release metadata is unlocked.
    pub(crate) fn collect_unused(&mut self) -> usize {
        let now = self.collection_time();
        let mut count = 0;
        while let Some(retired) = self.take_unused(now) {
            drop(retired);
            count += 1;
        }
        count
    }

    /// Detaches one expired source without allocation; disposal belongs outside the caller's lock.
    pub(crate) fn take_unused(&mut self, now: u32) -> Option<RetiredResource<K, T>> {
        while let Some(ticket) = self.releases.pop(now) {
            let Some(entry) = self.entries.get(ticket.index).and_then(Option::as_ref) else {
                continue;
            };
            if entry.ticket != ticket || entry.pin.has_consumers() {
                continue;
            }
            let entry = self.entries[ticket.index]
                .take()
                .unwrap_or_else(|| unreachable!("the cache alone mutates entries"));
            let (index_key, _) = self
                .indices
                .remove_entry(&entry.key)
                .unwrap_or_else(|| unreachable!("a retained source has its key index"));
            self.releases.unregister(ticket);
            return Some(RetiredResource {
                _entry: entry,
                _index_key: index_key,
            });
        }
        None
    }
}

#[cfg(test)]
#[path = "../../../tests/resource/lifetime.rs"]
mod tests;
