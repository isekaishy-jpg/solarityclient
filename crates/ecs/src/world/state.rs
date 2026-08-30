//! Active-world identity, lifecycle, and shared ECS state.

use shipyard::{EntityId, World};

use crate::movement::WorldTransform;
use crate::object::ObjectGuid;
use crate::player::{LocalPlayer, PlayerIdentity};

use super::{WorldMapId, registry::ObjectRegistry, types::WorldBootstrap};

/// Sole owner of active world entities and their server-GUID index.
pub struct ActiveWorld {
    map_id: WorldMapId,
    storage: World,
    objects: ObjectRegistry,
    local_player: EntityId,
}

impl ActiveWorld {
    /// Creates the active map and its initial local-player entity.
    #[must_use]
    pub fn enter(bootstrap: WorldBootstrap) -> Self {
        let (map_id, player_guid, player_name, position, orientation) = bootstrap.into_parts();
        let mut storage = World::new();
        let local_player = storage.add_entity((
            ObjectGuid::new(player_guid),
            PlayerIdentity::new(player_name),
            LocalPlayer,
            WorldTransform::new(position, orientation),
        ));
        let mut objects = ObjectRegistry::default();
        objects.insert(player_guid, local_player);
        Self {
            map_id,
            storage,
            objects,
            local_player,
        }
    }

    /// Returns the active Map.dbc identifier.
    #[must_use]
    pub const fn map_id(&self) -> WorldMapId {
        self.map_id
    }

    /// Returns the Shipyard entity controlled by this client.
    #[must_use]
    pub const fn local_player(&self) -> EntityId {
        self.local_player
    }

    /// Finds a loaded entity by its exact server GUID.
    #[must_use]
    pub fn entity_by_guid(&self, guid: u64) -> Option<EntityId> {
        self.objects.find(guid)
    }

    /// Returns immutable access to component storage for system dispatch.
    #[must_use]
    pub const fn storage(&self) -> &World {
        &self.storage
    }

    /// Returns mutable access to component storage for scheduled systems.
    #[must_use]
    pub const fn storage_mut(&mut self) -> &mut World {
        &mut self.storage
    }
}
