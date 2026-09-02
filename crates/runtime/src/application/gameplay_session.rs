//! Composition boundary between an accepted network login and ECS ownership.

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldMovementSpeeds, WorldMovementState,
    WorldStateError, WorldTransform,
};
use solarity_network::{
    InWorldSession, ObjectMovementUpdate, WorldObjectKind, WorldObjectUpdate,
    WorldObjectUpdateBatch,
};
use solarity_systems::{ObjectProjectionError, project_object_fields};
use thiserror::Error;

/// Failure while applying an authoritative object-update batch to ECS state.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GameplayUpdateError {
    /// The update violates active-world GUID lifecycle invariants.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// The decoded field table could not be projected into typed views.
    #[error(transparent)]
    Projection(#[from] ObjectProjectionError),
}

/// Live world transport paired with the ECS state it authoritatively seeded.
pub struct GameplaySession<S> {
    network: InWorldSession<S>,
    world: ActiveWorld,
}

impl<S> GameplaySession<S> {
    /// Transfers a verified selected-character result into ECS ownership.
    #[must_use]
    pub fn enter(network: InWorldSession<S>) -> Self {
        let location = network.location();
        let bootstrap = WorldBootstrap::new(
            WorldMapId::new(location.map_id()),
            network.character_guid(),
            network.character_name(),
            Vec3::new(location.x(), location.y(), location.z()),
            location.orientation(),
        );
        Self {
            network,
            world: ActiveWorld::enter(bootstrap),
        }
    }

    /// Returns the encrypted active-world network session.
    #[must_use]
    pub const fn network(&self) -> &InWorldSession<S> {
        &self.network
    }

    /// Returns mutable access to the active-world network session.
    #[must_use]
    pub const fn network_mut(&mut self) -> &mut InWorldSession<S> {
        &mut self.network
    }

    /// Returns the ECS-owned active world.
    #[must_use]
    pub const fn world(&self) -> &ActiveWorld {
        &self.world
    }

    /// Returns mutable access to the ECS-owned active world.
    #[must_use]
    pub const fn world_mut(&mut self) -> &mut ActiveWorld {
        &mut self.world
    }

    /// Separates async network ownership from main-thread ECS ownership after
    /// the authoritative bootstrap has been constructed.
    #[must_use]
    pub fn into_parts(self) -> (InWorldSession<S>, ActiveWorld) {
        (self.network, self.world)
    }

    /// Applies one decoded object-update batch in exact server order.
    ///
    /// Sparse update fields stream directly into dense component storage
    /// without allocating a second intermediate field collection.
    ///
    /// # Errors
    ///
    /// Returns [`GameplayUpdateError`] when a GUID lifecycle invariant is violated.
    pub fn apply_object_updates(
        &mut self,
        batch: &WorldObjectUpdateBatch,
    ) -> Result<(), GameplayUpdateError> {
        apply_object_updates(&mut self.world, batch)
    }
}

/// Applies a decoded batch to a main-thread-owned ECS world.
///
/// This free boundary lets the active socket move to an asynchronous task
/// without moving `ActiveWorld` or duplicating its GUID lifecycle rules.
pub(crate) fn apply_object_updates(
    world: &mut ActiveWorld,
    batch: &WorldObjectUpdateBatch,
) -> Result<(), GameplayUpdateError> {
    for update in batch.updates() {
        match update {
            WorldObjectUpdate::Values { guid, fields } => {
                world.update_fields(
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                project_object_fields(
                    world,
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
            }
            WorldObjectUpdate::Movement { guid, movement } => {
                if let Some(transform) = movement_transform(*movement) {
                    world.update_transform(*guid, transform)?;
                }
                if let Some(movement) = movement_state(*movement) {
                    world.update_movement(*guid, movement)?;
                }
            }
            WorldObjectUpdate::Create {
                guid,
                kind,
                movement,
                fields,
                ..
            } => {
                let existing = world.entity_by_guid(*guid);
                world.create_object(
                    *guid,
                    object_kind(*kind),
                    movement_transform(*movement),
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                // Stock skips the create movement block when the GUID already
                // resolves to a non-local object, retaining its live movement
                // state while refreshing the sparse field data.
                if (existing.is_none() || existing == Some(world.local_player()))
                    && let Some(movement) = movement_state(*movement)
                {
                    world.update_movement(*guid, movement)?;
                }
                project_object_fields(
                    world,
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
            }
            WorldObjectUpdate::OutOfRange(guids) => {
                for guid in guids {
                    world.remove_object(*guid)?;
                }
            }
            WorldObjectUpdate::Near(_) => {}
        }
    }
    Ok(())
}

fn movement_state(movement: ObjectMovementUpdate) -> Option<WorldMovementState> {
    Some(WorldMovementState::new(
        movement.movement_flags()?,
        WorldMovementSpeeds::new(movement.speeds()?.values()),
        movement.transport_guid(),
    ))
}

fn movement_transform(movement: ObjectMovementUpdate) -> Option<WorldTransform> {
    let [x, y, z] = movement.position()?;
    Some(WorldTransform::new(
        Vec3::new(x, y, z),
        movement.orientation()?,
    ))
}

const fn object_kind(kind: WorldObjectKind) -> ObjectKind {
    match kind {
        WorldObjectKind::Object => ObjectKind::Object,
        WorldObjectKind::Item => ObjectKind::Item,
        WorldObjectKind::Container => ObjectKind::Container,
        WorldObjectKind::Unit => ObjectKind::Unit,
        WorldObjectKind::Player => ObjectKind::Player,
        WorldObjectKind::GameObject => ObjectKind::GameObject,
        WorldObjectKind::DynamicObject => ObjectKind::DynamicObject,
        WorldObjectKind::Corpse => ObjectKind::Corpse,
    }
}
