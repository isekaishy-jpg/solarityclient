//! Entity registration and lookup ownership for the active world.

use std::collections::HashMap;

use shipyard::EntityId;

/// GUID-to-entity index owned beside the Shipyard world.
#[derive(Default)]
pub(crate) struct ObjectRegistry {
    by_guid: HashMap<u64, EntityId>,
}

impl ObjectRegistry {
    pub(crate) fn insert(&mut self, guid: u64, entity: EntityId) {
        self.by_guid.insert(guid, entity);
    }

    pub(crate) fn find(&self, guid: u64) -> Option<EntityId> {
        self.by_guid.get(&guid).copied()
    }

    pub(crate) fn remove(&mut self, guid: u64) -> Option<EntityId> {
        self.by_guid.remove(&guid)
    }
}
