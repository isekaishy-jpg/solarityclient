//! Entity registration and lookup ownership for the active world.

use std::collections::HashMap;

use shipyard::EntityId;

/// GUID-to-entity index owned beside the Shipyard world.
#[derive(Default)]
pub(crate) struct ObjectRegistry {
    by_guid: HashMap<u64, RegisteredObject>,
    first: Option<u64>,
    last: Option<u64>,
}

struct RegisteredObject {
    entity: EntityId,
    previous: Option<u64>,
    next: Option<u64>,
}

impl ObjectRegistry {
    pub(crate) fn insert(&mut self, guid: u64, entity: EntityId) {
        if let Some(current) = self.by_guid.get_mut(&guid) {
            current.entity = entity;
            return;
        }
        if let Some(last) = self.last.and_then(|last| self.by_guid.get_mut(&last)) {
            last.next = Some(guid);
        } else {
            self.first = Some(guid);
        }
        self.by_guid.insert(
            guid,
            RegisteredObject {
                entity,
                previous: self.last,
                next: None,
            },
        );
        self.last = Some(guid);
    }

    pub(crate) fn find(&self, guid: u64) -> Option<EntityId> {
        self.by_guid.get(&guid).map(|entry| entry.entity)
    }

    pub(crate) fn remove(&mut self, guid: u64) -> Option<EntityId> {
        let removed = self.by_guid.remove(&guid)?;
        if let Some(previous) = removed
            .previous
            .and_then(|guid| self.by_guid.get_mut(&guid))
        {
            previous.next = removed.next;
        } else {
            self.first = removed.next;
        }
        if let Some(next) = removed.next.and_then(|guid| self.by_guid.get_mut(&guid)) {
            next.previous = removed.previous;
        } else {
            self.last = removed.previous;
        }
        Some(removed.entity)
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (u64, EntityId)> + '_ {
        let mut next = self.first;
        std::iter::from_fn(move || {
            let guid = next?;
            let entry = self.by_guid.get(&guid)?;
            next = entry.next;
            Some((guid, entry.entity))
        })
    }
}
