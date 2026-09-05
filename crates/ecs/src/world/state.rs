//! Active-world identity, lifecycle, and shared ECS state.

use shipyard::{EntityId, World};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

use crate::game_object::{GameObjectMovement, GameObjectPresentation};
use crate::movement::{WorldMovementState, WorldTransform};
use crate::object::{ObjectFields, ObjectGuid, ObjectKind, ObjectPresentation};
use crate::player::{LocalPlayer, PlayerIdentity, PlayerMoney, PlayerProgression};
use crate::unit::{UnitIdentity, UnitPresentation, UnitVitals};
use crate::view::PlayerViewState;

use super::{WorldMapId, WorldObjectIdentity, registry::ObjectRegistry, types::WorldBootstrap};

static NEXT_WORLD_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Sole owner of active world entities and their server-GUID index.
pub struct ActiveWorld {
    identity: u64,
    map_id: WorldMapId,
    storage: World,
    objects: ObjectRegistry,
    local_player: EntityId,
}

impl ActiveWorld {
    /// Creates the active map and its initial local-player entity.
    #[must_use]
    pub fn enter(bootstrap: WorldBootstrap) -> Self {
        Self::enter_with_view(bootstrap, PlayerViewState::default())
    }

    /// Creates fresh replicated ownership with the session's retained camera view.
    ///
    /// World replacement detaches the old camera anchor (`0x006066E0`) and
    /// changes its base transform (`0x00607BD0`); it does not reconstruct the
    /// persistent camera or select the initial saved view again.
    #[must_use]
    pub fn enter_with_view(bootstrap: WorldBootstrap, view: PlayerViewState) -> Self {
        let (map_id, player_guid, player_name, position, orientation) = bootstrap.into_parts();
        let mut storage = World::new();
        let local_player = storage.add_entity((
            ObjectGuid::new(player_guid),
            PlayerIdentity::new(player_name),
            LocalPlayer,
            WorldTransform::new(position, orientation),
            view,
            ObjectFields::default(),
        ));
        let mut objects = ObjectRegistry::default();
        objects.insert(player_guid, local_player);
        Self {
            identity: NEXT_WORLD_IDENTITY.fetch_add(1, Ordering::Relaxed),
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

    /// Returns the admitted lifetime, including entity generation and world identity.
    #[must_use]
    pub fn object_identity(&self, guid: u64) -> Option<WorldObjectIdentity> {
        Some(WorldObjectIdentity {
            world: self.identity,
            entity: self.objects.find(guid)?,
            guid,
        })
    }

    /// Iterates visible GameObjects in admission order without allocating.
    /// Duplicate creates retain their position; removal/recreation enters at the end.
    pub fn visible_game_objects(&self) -> impl Iterator<Item = WorldObjectIdentity> + '_ {
        self.objects.entries().filter_map(|(guid, entity)| {
            self.storage
                .get::<&ObjectKind>(entity)
                .ok()
                .filter(|kind| ***kind == ObjectKind::GameObject)
                .map(|_| WorldObjectIdentity {
                    world: self.identity,
                    entity,
                    guid,
                })
        })
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

    /// Returns the local player's primary attributes after create projection.
    #[must_use]
    pub fn local_player_stats(&self) -> Option<crate::UnitStats> {
        self.storage
            .get::<&crate::UnitStats>(self.local_player)
            .map(|stats| **stats)
            .ok()
    }

    /// Returns the server-validated local character name seeded at world entry.
    #[must_use]
    pub fn local_player_identity(&self) -> Option<PlayerIdentity> {
        self.storage
            .get::<&PlayerIdentity>(self.local_player)
            .map(|identity| identity.clone())
            .ok()
    }

    /// Returns current and maximum local-player health and power values.
    #[must_use]
    pub fn local_player_vitals(&self) -> Option<UnitVitals> {
        self.storage
            .get::<&UnitVitals>(self.local_player)
            .map(|vitals| **vitals)
            .ok()
    }

    /// Returns the common presentation fields for any visible object.
    #[must_use]
    pub fn object_presentation(&self, guid: u64) -> Option<ObjectPresentation> {
        let entity = self.objects.find(guid)?;
        self.storage
            .get::<&ObjectPresentation>(entity)
            .map(|presentation| **presentation)
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

    /// Returns the admitted GameObject passenger and packed-rotation snapshot.
    #[must_use]
    pub fn game_object_movement(&self, guid: u64) -> Option<GameObjectMovement> {
        let entity = self.entity_by_guid(guid)?;
        self.storage
            .get::<&GameObjectMovement>(entity)
            .map(|value| **value)
            .ok()
    }

    /// Replaces the admitted GameObject movement snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when the server references an unknown GUID.
    pub fn update_game_object_movement(
        &mut self,
        guid: u64,
        movement: GameObjectMovement,
    ) -> Result<(), WorldStateError> {
        let entity = self.require_entity(guid)?;
        self.storage.add_component(entity, (movement,));
        Ok(())
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

    /// Consumes the native GameObject sequence seek in both typed and raw views.
    ///
    /// `0x0070D1E0` writes the upper half of absolute word 14 to `0xFFFF`
    /// without notifying another field update. The low dynamic flags survive.
    ///
    /// # Errors
    /// Returns [`WorldStateError`] for an absent object or missing field views.
    pub fn consume_game_object_sequence_progress(
        &self,
        guid: u64,
    ) -> Result<Option<u16>, WorldStateError> {
        let previous = self.set_game_object_sequence_progress(guid, u16::MAX)?;
        Ok((previous != u16::MAX).then_some(previous))
    }

    /// Writes a behavior-owned seek without generating a network-field notification.
    /// Returns the previous fraction; native reversal uses this same backing word.
    ///
    /// # Errors
    /// Returns [`WorldStateError`] for an absent object or missing field views.
    pub fn set_game_object_sequence_progress(
        &self,
        guid: u64,
        progress: u16,
    ) -> Result<u16, WorldStateError> {
        let entity = self.require_entity(guid)?;
        let mut presentation = self
            .storage
            .get::<&mut GameObjectPresentation>(entity)
            .map_err(|_| WorldStateError::MissingGameObjectPresentation { guid })?;
        let mut fields = self
            .storage
            .get::<&mut ObjectFields>(entity)
            .map_err(|_| WorldStateError::MissingObjectFields { guid })?;
        let dynamic = fields.get(14);
        let previous = (dynamic >> 16) as u16;
        let updated = (dynamic & 0xFFFF) | (u32::from(progress) << 16);
        **presentation = presentation.with_dynamic_word(updated);
        if previous != progress {
            fields.apply([(14, updated)]);
        }
        Ok(previous)
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
    /// A GameObject sequence consumer lacks its projected presentation fields.
    #[error("world object {guid:#018X} has no GameObject presentation")]
    MissingGameObjectPresentation {
        /// Object whose typed fields have not been projected.
        guid: u64,
    },
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
