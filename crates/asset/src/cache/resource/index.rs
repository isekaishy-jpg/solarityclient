//! Single-owner resource index visits release notifications instead of all sources.

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Weak},
};

use super::{
    ResourceCacheClock, ResourceLease,
    lease::Pin,
    releases::{Releases, Ticket},
};
use crate::AssetError;

/// Cache ownership is distinct from the current external consumer pin generation.
struct Entry<K, T> {
    key: K,
    value: Arc<T>,
    pin: Weak<Pin<T>>,
    ticket: Ticket,
}

/// Detached keys and payload ownership leave every index/accounting lock before disposal.
pub(crate) struct RetiredResource<K, T> {
    _entry: Entry<K, T>,
    _index_key: K,
}

/// Keys remain domain-defined; this index supplies no guessed age/eviction policy.
pub(crate) struct ResourceCache<K, T> {
    indices: HashMap<K, usize>,
    entries: Vec<Option<Entry<K, T>>>,
    releases: Arc<Releases>,
}

impl<K, T> Default for ResourceCache<K, T> {
    fn default() -> Self {
        Self {
            indices: HashMap::new(),
            entries: Vec::new(),
            releases: Arc::default(),
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

    /// Registers a namespace maintenance observer at the first load boundary.
    pub(crate) fn subscribe(&self, watcher: &Arc<std::sync::atomic::AtomicBool>) {
        self.releases.subscribe(watcher);
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
        if let Some(pin) = entry.pin.upgrade() {
            return Ok(Some(ResourceLease { pin }));
        }
        entry.ticket = self.releases.renew(entry.ticket)?;
        let lease = ResourceLease::cached(Arc::clone(&entry.value), &self.releases, entry.ticket);
        entry.pin = Arc::downgrade(&lease.pin);
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
        let ticket = self.releases.register()?;
        let lease = ResourceLease::cached(Arc::clone(&value), &self.releases, ticket);
        self.indices.insert(key.clone(), ticket.index);
        self.entries
            .resize_with(self.entries.len().max(ticket.index + 1), || None);
        self.entries[ticket.index] = Some(Entry {
            key,
            value,
            pin: Arc::downgrade(&lease.pin),
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
            if entry.ticket != ticket || entry.pin.strong_count() != 0 {
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
