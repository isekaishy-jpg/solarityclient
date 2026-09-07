//! Composition boundary between an accepted network login and ECS ownership.

mod game_object_notifications;
mod monster;
mod movement;

pub(crate) use monster::apply_monster_move;
pub(crate) use monster::prepare_monster_move;

use crate::application::game_object_behavior::GameObjectNotification;
use game_object_notifications::GameObjectUpdateMirrors;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, GameObjectMovement, GameObjectTransport, ObjectKind, WorldBootstrap, WorldMapId,
    WorldMovementContext, WorldMovementFall, WorldMovementSpeeds, WorldMovementState,
    WorldMovementTransport, WorldStateError, WorldTransform,
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
    /// The server spline cannot be admitted by the movement owner.
    #[error(transparent)]
    Spline(#[from] solarity_systems::MovementSplineError),
    /// A known living object lacks the movement state required by its command.
    #[error("movement command targets unit {guid:#018X} without movement state")]
    MissingMovement {
        /// Target GUID.
        guid: u64,
    },
    /// A living movement-only operation targeted a non-unit object category.
    #[error("living movement update targets {kind:?} object {guid:#018X}")]
    NonLivingMovement {
        /// Target GUID.
        guid: u64,
        /// Admitted category, which cannot own the native unit movement state.
        kind: ObjectKind,
    },
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

    /// Applies a batch at an explicit client receipt clock for deterministic
    /// movement playback and preserves the server's elapsed spline time.
    ///
    /// # Errors
    /// Returns an invalid world lifecycle, field projection, or movement error.
    pub fn apply_object_updates_at(
        &mut self,
        batch: &WorldObjectUpdateBatch,
        receipt_ms: u32,
    ) -> Result<(), GameplayUpdateError> {
        apply_object_updates_with(&mut self.world, batch, receipt_ms, &mut |_, _, _| Ok(()))
    }

    /// Applies a server-started path at an explicit receipt clock.
    /// Returns false for an unknown unit or a transport controller not yet owned.
    ///
    /// # Errors
    /// Rejects malformed movement geometry or a missing living movement state.
    pub fn apply_monster_move_at(
        &mut self,
        message: &solarity_network::MonsterMove,
        receipt_ms: u32,
        stop_distance_tolerance: f32,
    ) -> Result<bool, GameplayUpdateError> {
        apply_monster_move(
            &mut self.world,
            message,
            receipt_ms,
            stop_distance_tolerance,
        )
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
    apply_object_updates_with(
        world,
        batch,
        crate::platform::client_milliseconds(),
        &mut |_, _, _| Ok(()),
    )
}

/// Native packet processing applies all raw blocks before any field callback.
pub(crate) fn apply_object_updates_with<E: From<GameplayUpdateError>>(
    world: &mut ActiveWorld,
    batch: &WorldObjectUpdateBatch,
    receipt_ms: u32,
    notify: &mut impl FnMut(
        &ActiveWorld,
        solarity_ecs::WorldObjectIdentity,
        GameObjectNotification,
    ) -> Result<(), E>,
) -> Result<(), E> {
    let mirrors = apply_object_updates_raw(world, batch, receipt_ms).map_err(E::from)?;
    mirrors.dispatch(world, batch, notify)
}

fn apply_object_updates_raw(
    world: &mut ActiveWorld,
    batch: &WorldObjectUpdateBatch,
    receipt_ms: u32,
) -> Result<GameObjectUpdateMirrors, GameplayUpdateError> {
    let mut mirrors = GameObjectUpdateMirrors::default();
    for update in batch.updates() {
        match update {
            WorldObjectUpdate::Values { guid, fields } => {
                let previous = world.game_object_presentation(*guid);
                world.update_fields(
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                project_object_fields(
                    world,
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                mirrors.record(world, *guid, previous, false);
            }
            WorldObjectUpdate::Movement { guid, movement } => {
                // 0x004D6DA0 consumes the block but skips the local player's
                // echo. Remote blocks enter Unit_C's movement owner directly;
                // they carry neither GameObject rotation nor create flags.
                if *guid == world.local_player_guid()? {
                    continue;
                }
                match world.object_kind(*guid) {
                    Some(ObjectKind::Unit | ObjectKind::Player) => {}
                    Some(kind) => {
                        return Err(GameplayUpdateError::NonLivingMovement { guid: *guid, kind });
                    }
                    None => {
                        return Err(WorldStateError::UnknownObject { guid: *guid }.into());
                    }
                }
                if let Some(transform) = movement_transform(movement) {
                    world.update_transform(*guid, transform)?;
                }
                movement::install(world, *guid, movement, receipt_ms)?;
            }
            WorldObjectUpdate::Create {
                guid,
                kind,
                movement,
                fields,
                ..
            } => {
                let existing = world.entity_by_guid(*guid);
                let previous = world.game_object_presentation(*guid);
                world.create_object(
                    *guid,
                    object_kind(*kind),
                    movement_transform(movement),
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                // Stock skips the create movement block when the GUID already
                // resolves to a non-local object, retaining its live movement
                // state while refreshing the sparse field data.
                if existing.is_none() || existing == Some(world.local_player()) {
                    movement::install(world, *guid, movement, receipt_ms)?;
                }
                if (existing.is_none() || existing == Some(world.local_player()))
                    && *kind == WorldObjectKind::GameObject
                {
                    world.update_game_object_movement(
                        *guid,
                        game_object_movement(movement, receipt_ms),
                    )?;
                }
                project_object_fields(
                    world,
                    *guid,
                    fields.iter().map(|field| (field.index(), field.value())),
                )?;
                mirrors.record(world, *guid, previous, existing.is_none());
            }
            WorldObjectUpdate::OutOfRange(guids) => {
                for guid in guids {
                    world.remove_object(*guid)?;
                }
            }
            WorldObjectUpdate::Near(_) => {}
        }
    }
    Ok(mirrors)
}

/// Preserves the complete admitted living context at the network-to-ECS boundary.
fn movement_state(movement: &ObjectMovementUpdate) -> Option<WorldMovementState> {
    let context = movement.context()?;
    Some(WorldMovementState::new(
        movement.movement_flags()?,
        WorldMovementSpeeds::new(movement.speeds()?.values()),
        WorldMovementContext {
            timestamp_ms: context.timestamp_ms,
            transport: context.transport.map(|transport| WorldMovementTransport {
                guid: transport.guid,
                position: Vec3::from_array(transport.position),
                orientation: transport.orientation,
                time_ms: transport.time_ms,
                seat: transport.seat,
                interpolated_time_ms: transport.interpolated_time_ms,
            }),
            pitch_radians: context.pitch_radians,
            fall_time_ms: context.fall_time_ms,
            falling: context.falling.map(|falling| WorldMovementFall {
                vertical_speed: falling.vertical_speed,
                direction_sin: falling.direction_sin,
                direction_cos: falling.direction_cos,
                horizontal_speed: falling.horizontal_speed,
            }),
            spline_elevation: context.spline_elevation,
        },
    ))
}

fn movement_transform(movement: &ObjectMovementUpdate) -> Option<WorldTransform> {
    let [x, y, z] = movement.position()?;
    Some(WorldTransform::new(
        Vec3::new(x, y, z),
        movement.orientation()?,
    ))
}

fn game_object_movement(movement: &ObjectMovementUpdate, receipt_ms: u32) -> GameObjectMovement {
    let state = GameObjectMovement::new(
        movement.packed_rotation().unwrap_or(0),
        movement
            .position_transport()
            .filter(|transport| transport.guid != 0)
            .map(|transport| GameObjectTransport {
                guid: transport.guid,
                position: Vec3::from_array(transport.position),
                orientation: transport.orientation,
            }),
    );
    movement
        .transport_progress_ms()
        .map_or(state, |progress_ms| {
            state.with_transport_clock(progress_ms, receipt_ms)
        })
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
