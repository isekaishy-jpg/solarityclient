//! Single-owner resource index visits release notifications instead of all sources.

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Weak},
};

use super::{
    ResourceLease,
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
    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// A hit retains source identity but never authorizes a live scene instance.
    pub(crate) fn get(&mut self, key: &K) -> Option<ResourceLease<T>> {
        let index = *self.indices.get(key)?;
        let entry = self.entries[index]
            .as_mut()
            .unwrap_or_else(|| unreachable!("registered keys own their entry"));
        if let Some(pin) = entry.pin.upgrade() {
            return Some(ResourceLease { pin });
        }
        let lease = ResourceLease::cached(Arc::clone(&entry.value), &self.releases, entry.ticket);
        entry.pin = Arc::downgrade(&lease.pin);
        Some(lease)
    }

    /// Installs one successfully decoded source. A preexisting generation retains authority.
    pub(crate) fn insert(&mut self, key: K, value: T) -> Result<ResourceLease<T>, AssetError> {
        if let Some(existing) = self.get(&key) {
            return Ok(existing);
        }
        let ticket = self.releases.register()?;
        let value = Arc::new(value);
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
        let mut removed = 0;
        while let Some(ticket) = self.releases.pop() {
            let Some(entry) = self.entries.get(ticket.index).and_then(Option::as_ref) else {
                continue;
            };
            if entry.ticket != ticket || entry.pin.strong_count() != 0 {
                continue;
            }
            let entry = self.entries[ticket.index]
                .take()
                .unwrap_or_else(|| unreachable!("the cache alone mutates entries"));
            self.indices.remove(&entry.key);
            self.releases.unregister(ticket);
            drop(entry);
            removed += 1;
        }
        removed
    }
}

#[cfg(test)]
#[path = "../../../tests/resource/lifetime.rs"]
mod tests;
