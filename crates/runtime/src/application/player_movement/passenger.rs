//! Active-mover parent changes, retained coordinate state, and transport clocks.

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, WorldMovementState, WorldMovementTransport, WorldObjectIdentity, WorldTransform,
};
use solarity_network::WorldMovementKind;
use solarity_systems::{MovementFallState, MovementTransportChange, MovementTransportFrame};

use super::{
    LocalMovement, LocalMovementGeometry, MovementCommand, MovementPhase, RuntimePlayerMovement,
    RuntimePlayerMovementError,
};
use crate::application::{RuntimeGameObjectPresentation, RuntimeMovementGeometry};

#[cfg(test)]
#[path = "../../../tests/application/player_passenger.rs"]
mod tests;

/// One borrowed scene's parent snapshot, always keyed by its full entity lifetime.
#[derive(Clone, Copy)]
pub(super) struct PassengerParent {
    pub identity: WorldObjectIdentity,
    pub frame: MovementTransportFrame,
    pub time_ms: Option<u32>,
    pub can_board: bool,
}

impl PassengerParent {
    /// A missing object or matrix is not a usable collision-contact parent.
    pub fn resolve(
        geometry: &RuntimeMovementGeometry<'_>,
        guid: u64,
    ) -> Result<Option<Self>, RuntimePlayerMovementError> {
        let Some(identity) = geometry.passenger_identity(guid) else {
            return Ok(None);
        };
        let Some(frame) = geometry.passenger_frame(identity)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            identity,
            frame,
            time_ms: geometry.passenger_time_ms(identity),
            can_board: geometry.can_board(identity),
        }))
    }
}

/// TLS movement-context +0x130/+0x138/+0x134, independent of unit blend time.
#[derive(Clone, Copy, Default)]
pub(super) struct PassengerClock {
    current: u32,
    previous: u32,
    pending: bool,
}

impl PassengerClock {
    /// Native 6E8F70 publishes even an unchanged phase, retaining the former value.
    pub fn publish(&mut self, time_ms: u32) {
        self.previous = self.current;
        self.current = time_ms;
    }

    /// 6EC400 marks interpolation only when replacing a nonzero old parent.
    fn switched(&mut self) {
        self.pending = self.current != self.previous;
    }

    /// 987140 consumes this flag only when it actually serializes a transport block.
    pub fn serialized(&mut self) {
        self.pending = false;
    }
}

impl RuntimePlayerMovement {
    /// Runs transport destruction while the outgoing generation still owns its matrix.
    /// A world replacement or an authoritative change to another parent already
    /// supersedes this link and must not produce a stale detach notification.
    pub(in crate::application) fn retire_passenger(
        &mut self,
        world: Option<&ActiveWorld>,
        objects: &RuntimeGameObjectPresentation,
        now_ms: u32,
    ) -> Result<(), RuntimePlayerMovementError> {
        let Some(world) = world else {
            return Ok(());
        };
        let Some(owner) = self.owner.as_mut() else {
            return Ok(());
        };
        if world.object_identity(owner.identity.guid()) != Some(owner.identity) {
            return Ok(());
        }
        let Some(parent) = owner.passenger else {
            return Ok(());
        };
        let Some(frame) = objects.retiring_passenger_frame(world, parent.identity)? else {
            return Ok(());
        };
        let Some(movement) = world.movement_state(owner.identity.guid()) else {
            return Ok(());
        };
        let Some(transport) = movement
            .context()
            .transport
            .filter(|transport| transport.guid == parent.identity.guid())
        else {
            return Ok(());
        };
        let transform = world.local_player_transform()?;
        if owner.published != (transform, movement) {
            // A correction received before removal has already changed the
            // passenger's local pose. Consume it before the destruction callback.
            let mut corrected = LocalMovement::new_on_parent(
                owner.identity,
                transform,
                movement,
                owner.time_ms,
                parent,
                transport,
            )?;
            corrected.active = owner.active;
            corrected.client_control = owner.client_control;
            corrected.stand_state = owner.stand_state;
            corrected.camera = owner.camera;
            corrected.passenger_clock = owner.passenger_clock;
            *owner = corrected;
        }
        owner.flags &= !0x0800_0000; // 9872B0 precedes the queued event and unlink.
        owner.rebase_passenger(frame.exit_change())?;
        owner.passenger = None;
        owner.passenger_seat = -1;
        if owner.active {
            // 6ECCF0 inserts event 9 with 6EC090's stable wrapping-time order;
            // 6F0C70 immediately sends the forced leave before that event runs.
            let index = self
                .commands
                .iter()
                .position(|command| (now_ms.wrapping_sub(command.timestamp_ms()) as i32) < 0)
                .unwrap_or(self.commands.len());
            self.commands.insert(
                index,
                MovementCommand::SupportRecheck {
                    timestamp_ms: now_ms,
                },
            );
            let saved = owner.time_ms;
            owner.time_ms = now_ms;
            owner.emit(WorldMovementKind::ChangeTransport, &mut self.output)?;
            owner.time_ms = saved;
        }
        Ok(())
    }
}

impl LocalMovement {
    /// Seeds an authoritative passenger from its transmitted local coordinates.
    /// Loading keeps ownership pending until the parent matrix is resident.
    pub(super) fn new_in_geometry<G: LocalMovementGeometry>(
        identity: WorldObjectIdentity,
        transform: WorldTransform,
        movement: WorldMovementState,
        time_ms: u32,
        geometry: &G,
    ) -> Result<Option<Self>, RuntimePlayerMovementError> {
        let mut context = movement.context();
        context.transport = None;
        let unparented = WorldMovementState::new(
            movement.flags() & !(0x400_u64 << 32),
            movement.speeds(),
            context,
        );
        let Some(transport) = movement
            .context()
            .transport
            .filter(|transport| transport.guid != 0)
        else {
            let mut owner = Self::new(identity, transform, unparented, time_ms)?;
            owner.published = (transform, movement);
            return Ok(Some(owner));
        };
        let Some(parent) = geometry.passenger(transport.guid)? else {
            return Ok(None);
        };
        Self::new_on_parent(identity, transform, movement, time_ms, parent, transport).map(Some)
    }

    /// Shares authoritative local-pose admission with the pre-retirement callback.
    pub(super) fn new_on_parent(
        identity: WorldObjectIdentity,
        transform: WorldTransform,
        movement: WorldMovementState,
        time_ms: u32,
        parent: PassengerParent,
        transport: WorldMovementTransport,
    ) -> Result<Self, RuntimePlayerMovementError> {
        let mut context = movement.context();
        context.transport = None;
        let local = WorldTransform::new(transport.position, transport.orientation);
        let unparented = WorldMovementState::new(
            movement.flags() & !(0x400_u64 << 32),
            movement.speeds(),
            context,
        );
        let mut owner = Self::new(identity, local, unparented, time_ms)?;
        owner.passenger = Some(parent);
        owner.passenger_seat = transport.seat;
        owner.published = (transform, movement);
        Ok(owner)
    }

    /// Scene motion changes the world projection while local analytic anchors stay fixed.
    pub(super) fn refresh_passenger<G: LocalMovementGeometry>(
        &mut self,
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        if let Some(old) = self.passenger {
            self.passenger = geometry
                .passenger(old.identity.guid())?
                .filter(|parent| parent.identity == old.identity);
            // 6EC400's missing-parent branch clears the GUID without applying a
            // stale matrix. Object destruction normally detaches before retirement.
            if self.passenger.is_none() {
                self.passenger_seat = -1;
            }
            if self.active
                && !self.remote
                && let Some(time) = self.passenger.and_then(|parent| parent.time_ms)
            {
                self.passenger_clock.publish(time);
            }
        }
        geometry.set_passenger_frame(self.passenger.map(|parent| parent.frame));
        Ok(())
    }

    /// Applies 6EC7B0's active-owner gate, retention predicate, and 6EC400 admission.
    pub(super) fn contact_passenger<G: LocalMovementGeometry>(
        &mut self,
        guid: u64,
        geometry: &mut G,
    ) -> Result<bool, RuntimePlayerMovementError> {
        if !self.active || self.remote {
            return Ok(false);
        }
        let old = self.passenger;
        if guid == 0
            && let Some(parent) = old
            && geometry.retains_passenger(parent.identity, self.position)?
        {
            return Ok(false);
        }
        if old.is_some_and(|parent| parent.identity.guid() == guid) && self.passenger_seat == -1 {
            return Ok(false);
        }
        let next = if guid == 0 {
            None
        } else {
            let Some(parent) = geometry.passenger(guid)?.filter(|parent| parent.can_board) else {
                // A rejected nonzero candidate does not trigger the leave path.
                return Ok(false);
            };
            Some(parent)
        };
        if old.is_none() && next.is_none() {
            return Ok(false);
        }
        self.passenger_seat = -1;
        if old.map(|parent| parent.identity) == next.map(|parent| parent.identity) {
            // 762E00 compares parent GUIDs for notification and interval exit;
            // a seat-only update does not count as a coordinate-system change.
            return Ok(false);
        }
        if let Some(parent) = old {
            self.rebase_passenger(parent.frame.exit_change())?;
        }
        if let Some(parent) = next {
            self.rebase_passenger(parent.frame.entry_change())?;
            // 98BA20 -> 74B340 -> 711AB0 publishes the new behavior's clock
            // immediately. First boarding leaves the optional-clock flag alone.
            if let Some(time) = parent.time_ms {
                self.passenger_clock.publish(time);
            }
            if old.is_some() {
                self.passenger_clock.switched();
            }
        }
        self.passenger = next;
        geometry.set_passenger_frame(next.map(|parent| parent.frame));
        Ok(true)
    }

    /// 98B850 changes every retained coordinate lane without restarting analytic time.
    fn rebase_passenger(
        &mut self,
        change: MovementTransportChange,
    ) -> Result<(), RuntimePlayerMovementError> {
        let old_position = self.position;
        self.retained_launch_height =
            change.launch_height(old_position, self.retained_launch_height);
        self.anchor = change.position(self.anchor);
        self.position = change.position(old_position);
        self.orientation = change.orientation(self.orientation);
        self.ground = self.ground.rebased(change)?;
        self.yaw = self.yaw.rebased(change)?;
        self.phase = match self.phase {
            MovementPhase::Ground { step_anchor } => MovementPhase::Ground {
                step_anchor: step_anchor.map(|height| change.step_height(old_position, height)),
            },
            MovementPhase::Fall(fall) => {
                let mut state = fall.snapshot();
                state.position = self.position;
                state.launch_height = self.retained_launch_height;
                state.direction = change.direction(state.direction);
                state.horizontal_direction =
                    MovementTransportChange::horizontal_direction(state.direction);
                MovementPhase::Fall(MovementFallState::new(state)?)
            }
        };
        Ok(())
    }

    /// World-space facing is used by the camera, rendering, and ordinary wire fields.
    pub(super) fn world_orientation(&self) -> f32 {
        self.passenger.map_or(self.orientation, |parent| {
            parent.frame.world_orientation(self.orientation)
        })
    }

    /// The public transform is a projection; collision and fall state remain local.
    pub(super) fn world_position(&self) -> Vec3 {
        self.passenger.map_or(self.position, |parent| {
            parent.frame.world_position(self.position)
        })
    }

    /// Also represents 987140's explicit GUID-zero ChangeTransport leave block.
    pub(super) fn passenger_snapshot(&self) -> WorldMovementTransport {
        // Remote owners retain the received metadata. Only the active mover
        // publishes the TLS transport clock through 6E8F70/987140.
        let received = self.remote.then_some(self.context.transport).flatten();
        WorldMovementTransport {
            guid: self.passenger.map_or(0, |parent| parent.identity.guid()),
            position: self.position,
            orientation: self.orientation,
            time_ms: received.map_or(self.passenger_clock.current, |parent| parent.time_ms),
            seat: self.passenger_seat,
            interpolated_time_ms: if let Some(received) = received {
                received.interpolated_time_ms
            } else {
                self.passenger_clock
                    .pending
                    .then_some(self.passenger_clock.previous)
            },
        }
    }
}
