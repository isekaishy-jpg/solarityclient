//! Active-world identity, lifecycle, and shared ECS state.

use shipyard::{EntityId, World};
use thiserror::Error;

use crate::movement::WorldTransform;
use crate::object::{ObjectFields, ObjectGuid, ObjectKind};
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
            ObjectFields::default(),
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

    /// Creates a visible object or enriches the pre-seeded local player.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] for a duplicate non-local create update.
    pub fn create_object<I>(
        &mut self,
        guid: u64,
        kind: ObjectKind,
        transform: Option<WorldTransform>,
        fields: I,
    ) -> Result<EntityId, WorldStateError>
    where
        I: IntoIterator<Item = (u16, u32)>,
    {
        let mut object_fields = ObjectFields::default();
        object_fields.apply(fields);
        if let Some(entity) = self.objects.find(guid) {
            if entity != self.local_player {
                return Err(WorldStateError::DuplicateObject { guid });
            }
            self.storage.add_component(entity, (kind, object_fields));
            if let Some(transform) = transform {
                self.storage.add_component(entity, (transform,));
            }
            return Ok(entity);
        }
        let entity = self
            .storage
            .add_entity((ObjectGuid::new(guid), kind, object_fields));
        if let Some(transform) = transform {
            self.storage.add_component(entity, (transform,));
        }
        self.objects.insert(guid, entity);
        Ok(entity)
    }

    /// Applies sparse field words to an existing visible object.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when the server references an unknown GUID.
    pub fn update_fields<I>(&mut self, guid: u64, fields: I) -> Result<(), WorldStateError>
    where
        I: IntoIterator<Item = (u16, u32)>,
    {
        let entity = self.require_entity(guid)?;
        let fields = fields.into_iter();
        let mut object_fields = self
            .storage
            .get::<&mut ObjectFields>(entity)
            .map_err(|_| WorldStateError::MissingObjectFields { guid })?;
        object_fields.apply(fields);
        Ok(())
    }

    /// Replaces the authoritative transform for an existing object.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when the server references an unknown GUID.
    pub fn update_transform(
        &mut self,
        guid: u64,
        transform: WorldTransform,
    ) -> Result<(), WorldStateError> {
        let entity = self.require_entity(guid)?;
        self.storage.add_component(entity, (transform,));
        Ok(())
    }

    /// Removes an object that left server update range.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] for an unknown GUID or an attempt to remove
    /// the controlled player through an out-of-range update.
    pub fn remove_object(&mut self, guid: u64) -> Result<(), WorldStateError> {
        let entity = self.require_entity(guid)?;
        if entity == self.local_player {
            return Err(WorldStateError::LocalPlayerOutOfRange { guid });
        }
        self.objects.remove(guid);
        if !self.storage.delete_entity(entity) {
            return Err(WorldStateError::UnknownObject { guid });
        }
        Ok(())
    }

    fn require_entity(&self, guid: u64) -> Result<EntityId, WorldStateError> {
        self.objects
            .find(guid)
            .ok_or(WorldStateError::UnknownObject { guid })
    }
}

/// Invalid application of an authoritative object update.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldStateError {
    /// A packet referenced an object that has not been created.
    #[error("world update references unknown object {guid:#018X}")]
    UnknownObject {
        /// Referenced GUID.
        guid: u64,
    },
    /// A non-local object was created twice without leaving range.
    #[error("world update creates duplicate object {guid:#018X}")]
    DuplicateObject {
        /// Duplicate GUID.
        guid: u64,
    },
    /// The server attempted to remove the controlled player as out of range.
    #[error("world update marks local player {guid:#018X} out of range")]
    LocalPlayerOutOfRange {
        /// Controlled player GUID.
        guid: u64,
    },
    /// An indexed entity lost its required dense update-field component.
    #[error("world object {guid:#018X} has no update-field storage")]
    MissingObjectFields {
        /// Corrupt entity GUID.
        guid: u64,
    },
}
