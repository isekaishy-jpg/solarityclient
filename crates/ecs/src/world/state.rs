//! Active-world identity, lifecycle, and shared ECS state.

use shipyard::{EntityId, World};
use thiserror::Error;

use crate::game_object::GameObjectPresentation;
use crate::movement::{WorldMovementState, WorldTransform};
use crate::object::{ObjectFields, ObjectGuid, ObjectKind};
use crate::player::{LocalPlayer, PlayerIdentity, PlayerMoney, PlayerProgression};
use crate::unit::{UnitIdentity, UnitPresentation};
use crate::view::PlayerViewState;

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
            PlayerViewState::default(),
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

    /// Returns the exact server GUID of the controlled player.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError::MissingLocalPlayerGuid`] if ECS storage no
    /// longer satisfies the active-world bootstrap invariant.
    pub fn local_player_guid(&self) -> Result<u64, WorldStateError> {
        self.storage
            .get::<&ObjectGuid>(self.local_player)
            .map(|guid| guid.value())
            .map_err(|_| WorldStateError::MissingLocalPlayerGuid)
    }

    /// Returns the authoritative transform used by camera and terrain systems.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError::MissingLocalPlayerTransform`] if ECS storage
    /// no longer satisfies the active-world bootstrap invariant.
    pub fn local_player_transform(&self) -> Result<WorldTransform, WorldStateError> {
        self.storage
            .get::<&WorldTransform>(self.local_player)
            .map(|transform| **transform)
            .map_err(|_| WorldStateError::MissingLocalPlayerTransform)
    }

    /// Returns the durable renderer-independent local camera state.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError::MissingLocalPlayerView`] if ECS storage no
    /// longer satisfies the active-world bootstrap invariant.
    pub fn local_player_view(&self) -> Result<PlayerViewState, WorldStateError> {
        self.storage
            .get::<&PlayerViewState>(self.local_player)
            .map(|view| **view)
            .map_err(|_| WorldStateError::MissingLocalPlayerView)
    }

    /// Returns projected presentation fields after the create update arrives.
    #[must_use]
    pub fn local_player_presentation(&self) -> Option<UnitPresentation> {
        self.storage
            .get::<&UnitPresentation>(self.local_player)
            .map(|presentation| **presentation)
            .ok()
    }

    /// Returns private player coinage after its authoritative field arrives.
    #[must_use]
    pub fn local_player_money(&self) -> Option<PlayerMoney> {
        self.storage
            .get::<&PlayerMoney>(self.local_player)
            .map(|money| **money)
            .ok()
    }

    /// Returns private player XP after its authoritative fields arrive.
    #[must_use]
    pub fn local_player_progression(&self) -> Option<PlayerProgression> {
        self.storage
            .get::<&PlayerProgression>(self.local_player)
            .map(|progression| **progression)
            .ok()
    }

    /// Returns the local player's public unit identity after create projection.
    #[must_use]
    pub fn local_player_unit_identity(&self) -> Option<UnitIdentity> {
        self.storage
            .get::<&UnitIdentity>(self.local_player)
            .map(|identity| **identity)
            .ok()
    }

    /// Finds a loaded entity by its exact server GUID.
    #[must_use]
    pub fn entity_by_guid(&self, guid: u64) -> Option<EntityId> {
        self.objects.find(guid)
    }

    /// Returns the create-time object category for a loaded GUID.
    #[must_use]
    pub fn object_kind(&self, guid: u64) -> Option<ObjectKind> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&ObjectKind>(entity)
            .map(|kind| **kind)
            .ok()
    }

    /// Returns the latest complete living movement state for a loaded GUID.
    #[must_use]
    pub fn movement_state(&self, guid: u64) -> Option<WorldMovementState> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&WorldMovementState>(entity)
            .map(|movement| **movement)
            .ok()
    }

    /// Returns the transport parent named by the controlled player's movement.
    #[must_use]
    pub fn local_player_transport_guid(&self) -> Option<u64> {
        self.movement_state(self.local_player_guid().ok()?)
            .and_then(WorldMovementState::transport_guid)
            .filter(|guid| *guid != 0)
    }

    /// Reports whether the controlled player's transport parent is admitted.
    #[must_use]
    pub fn is_local_player_transport_admitted(&self) -> bool {
        self.local_player_transport_guid()
            .is_none_or(|guid| self.object_kind(guid) == Some(ObjectKind::GameObject))
    }

    /// Reports whether the admitted transport owns complete display inputs.
    #[must_use]
    pub fn is_local_player_transport_presentable(&self) -> bool {
        self.local_player_transport_guid().is_none_or(|guid| {
            self.object_kind(guid) == Some(ObjectKind::GameObject)
                && self.object_transform(guid).is_some()
                && self
                    .game_object_presentation(guid)
                    .is_some_and(|presentation| presentation.display_id() != 0)
        })
    }

    /// Returns every visible unit/player GUID in deterministic identifier order.
    #[must_use]
    pub fn visible_unit_guids(&self) -> Vec<u64> {
        let mut guids = self
            .objects
            .entries()
            .filter_map(|(guid, entity)| {
                self.storage
                    .get::<&ObjectKind>(entity)
                    .ok()
                    .filter(|kind| matches!(**kind, ObjectKind::Unit | ObjectKind::Player))
                    .map(|_kind| guid)
            })
            .collect::<Vec<_>>();
        guids.sort_unstable();
        guids
    }

    /// Returns the authoritative transform for any visible object.
    #[must_use]
    pub fn object_transform(&self, guid: u64) -> Option<WorldTransform> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&WorldTransform>(entity)
            .map(|transform| **transform)
            .ok()
    }

    /// Returns projected unit presentation state for any visible unit/player.
    #[must_use]
    pub fn unit_presentation(&self, guid: u64) -> Option<UnitPresentation> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&UnitPresentation>(entity)
            .map(|presentation| **presentation)
            .ok()
    }

    /// Returns the display identity projected for one visible game object.
    #[must_use]
    pub fn game_object_presentation(&self, guid: u64) -> Option<GameObjectPresentation> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&GameObjectPresentation>(entity)
            .map(|presentation| **presentation)
            .ok()
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

    /// Creates a visible object or refreshes an already-known stock object.
    ///
    /// Stock keeps recently disabled objects available for lazy cleanup and
    /// treats a repeated create as sparse field data for that same object. The
    /// local player is the one exception in this representation: world entry
    /// pre-seeds it before its authoritative create arrives, so that first
    /// create also supplies its kind and transform.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when an existing object's required field
    /// storage is missing.
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
        if let Some(entity) = self.objects.find(guid) {
            {
                let mut object_fields = self
                    .storage
                    .get::<&mut ObjectFields>(entity)
                    .map_err(|_| WorldStateError::MissingObjectFields { guid })?;
                object_fields.apply(fields);
            }

            if entity == self.local_player {
                self.storage.add_component(entity, (kind,));
                if let Some(transform) = transform {
                    self.storage.add_component(entity, (transform,));
                }
            }
            return Ok(entity);
        }
        let mut object_fields = ObjectFields::default();
        object_fields.apply(fields);
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

    /// Replaces the complete authoritative living movement state.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when the server references an unknown GUID.
    pub fn update_movement(
        &mut self,
        guid: u64,
        movement: WorldMovementState,
    ) -> Result<(), WorldStateError> {
        let entity = self.require_entity(guid)?;
        self.storage.add_component(entity, (movement,));
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
    /// The controlled player lost the transform seeded during world entry.
    #[error("active world's local player has no authoritative transform")]
    MissingLocalPlayerTransform,
    /// The controlled player lost the GUID seeded during world entry.
    #[error("active world's local player has no server GUID")]
    MissingLocalPlayerGuid,
    /// The controlled player lost its persistent camera state.
    #[error("active world's local player has no view state")]
    MissingLocalPlayerView,
}
