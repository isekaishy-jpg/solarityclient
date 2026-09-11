//! Local server paths retain the input, camera, packet, and passenger owners.

use super::spline::{SplineStep, prepare_passenger_path, project};
use super::*;

#[cfg(test)]
#[path = "../../../tests/application/local_server_path.rs"]
mod tests;
use solarity_systems::MovementSpline;

impl LocalMovement {
    pub(super) fn synchronize_passenger_input(
        &mut self,
        world: &ActiveWorld,
        frames: &super::super::unit_passenger::UnitPassengerFrames,
        animations: &super::super::unit_animation::UnitAnimationScene,
    ) {
        self.animation_events.retain(|event| {
            if matches!(event.kind, UnitMovementAnimationEventKind::Passenger { .. }) {
                animations.admit_movement_passenger(world, frames, *event);
                false
            } else {
                true
            }
        });
        let blocked = animations.passenger_input_blocked(self.identity);
        if blocked != self.passenger_input_blocked {
            self.passenger_input_blocked = blocked;
            // 747CA0 refreshes physical held keys on return to phase 0 or 3.
            self.input_refresh_pending = true;
        }
    }

    pub(super) fn receive_server_event<G: LocalMovementGeometry>(
        &mut self,
        event: remote::inbox::RemoteMovementInput,
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        use remote::inbox::RemoteMovementInput;
        match event {
            RemoteMovementInput::Baseline {
                transform,
                movement,
                spline,
                ..
            } => {
                let previous = self.snapshot();
                if self.replace_authoritative(
                    transform,
                    movement,
                    spline.map(|path| *path),
                    self.time_ms,
                    geometry,
                )? {
                    self.notify_server_parent(
                        previous,
                        self.passenger.map(|parent| parent.identity),
                        false,
                    );
                    self.refresh_passenger(geometry)?;
                    self.input_refresh_pending = true;
                }
            }
            RemoteMovementInput::Path {
                message,
                receipt_ms,
                stop_distance_tolerance,
            } => {
                self.receive_server_path(
                    &message,
                    receipt_ms,
                    stop_distance_tolerance,
                    geometry,
                    output,
                )?;
            }
            // Ordinary active-mover packets are rejected by receipt admission.
            // Control selection owns the local player's inactive command policy.
            RemoteMovementInput::Command { .. } => {}
        }
        Ok(())
    }

    pub(super) fn path_active(&self) -> bool {
        self.published
            .1
            .spline()
            .is_some_and(|path| path.flags & 0x400 == 0)
    }

    /// An object baseline replaces physical state without replacing local controls.
    pub(super) fn replace_authoritative<G: LocalMovementGeometry>(
        &mut self,
        transform: WorldTransform,
        movement: WorldMovementState,
        path: Option<MovementSpline>,
        time_ms: u32,
        geometry: &G,
    ) -> Result<bool, RuntimePlayerMovementError> {
        let Some(mut next) =
            Self::new_in_geometry(self.identity, transform, movement, time_ms, geometry)?
        else {
            return Ok(false);
        };
        next.active = self.active;
        next.client_control = self.client_control;
        next.stand_state = self.stand_state;
        next.camera = self.camera;
        next.passenger_clock = self.passenger_clock;
        next.previous_water_depth = self.previous_water_depth;
        next.is_swimming = self.is_swimming;
        next.ground_normal = self.ground_normal;
        next.heartbeat_ms = self.heartbeat_ms;
        next.scene_collision = self.scene_collision;
        next.passenger_input_blocked = self.passenger_input_blocked;
        next.path = path;
        next.animation_events = std::mem::take(&mut self.animation_events);
        next.water_splashes = std::mem::take(&mut self.water_splashes);
        next.tutorials = std::mem::take(&mut self.tutorials);
        *self = next;
        Ok(true)
    }

    pub(super) fn receive_server_path<G: LocalMovementGeometry>(
        &mut self,
        message: &solarity_network::MonsterMove,
        receipt_ms: u32,
        stop_distance_tolerance: f32,
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        let previous = self.snapshot();
        let stopped = super::super::gameplay_session::stop_before_path(previous.1);
        if stopped != previous.1 {
            self.replace_authoritative(previous.0, stopped, None, self.time_ms, geometry)?;
        }
        let Some(prepared) = prepare_passenger_path(
            self.snapshot(),
            self.passenger,
            message,
            receipt_ms,
            stop_distance_tolerance,
            geometry,
            |guid| geometry.target_position(guid),
        )?
        else {
            return Ok(());
        };
        let old_parent = self.passenger.map(|parent| parent.identity);
        let new_parent = prepared.parent.map(|parent| parent.identity);
        let seat_changed = prepared
            .movement
            .context()
            .transport
            .is_some_and(|transport| transport.seat != self.passenger_seat);
        // 6EC400 also returns success for a seat-only change; 6F0C70 sends
        // ChangeTransport in that case without switching the parent clock.
        if old_parent != new_parent || seat_changed {
            let retained_path = self.path.take();
            self.replace_authoritative(
                prepared.attachment.0,
                prepared.attachment.1,
                retained_path,
                self.time_ms,
                geometry,
            )?;
            self.refresh_passenger(geometry)?;
            if old_parent.is_some() && old_parent != new_parent {
                self.passenger_clock.switched();
            }
            if self.active {
                self.emit(WorldMovementKind::ChangeTransport, output)?;
            }
        }
        if !self.replace_authoritative(
            prepared.world,
            prepared.movement,
            prepared.spline,
            self.time_ms,
            geometry,
        )? {
            return Ok(());
        }
        self.notify_server_parent(
            previous,
            prepared.parent.map(|parent| parent.identity),
            true,
        );
        self.refresh_passenger(geometry)?;
        self.input_refresh_pending = true;
        self.notify_animation(UnitMovementAnimationEventKind::Changed);
        Ok(())
    }

    pub(super) fn notify_server_parent(
        &mut self,
        previous: (WorldTransform, WorldMovementState),
        parent: Option<WorldObjectIdentity>,
        animated: bool,
    ) {
        let key = |movement: WorldMovementState| {
            movement
                .context()
                .transport
                .filter(|transport| transport.guid != 0)
                .map(|transport| (transport.guid, transport.seat))
        };
        if key(previous.1) != key(self.snapshot().1) {
            self.notify_animation(UnitMovementAnimationEventKind::Passenger {
                previous_transform: previous.0,
                previous: key(previous.1),
                parent,
                animated,
            });
        }
    }

    pub(super) fn advance_server_path<G: LocalMovementGeometry>(
        &mut self,
        duration: u32,
        dimensions: [f32; 3],
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        let (_, movement) = self.snapshot();
        let current = WorldTransform::new(self.position, self.orientation);
        let parent = self.passenger;
        let Some(path) = &mut self.path else {
            return Ok(());
        };
        let next_ms = self.time_ms.wrapping_add(duration);
        let (local, movement) = path.advance_movement(next_ms, movement, current, |guid| {
            geometry.target_position(guid).map(|position| {
                parent.map_or(position, |parent| parent.frame.local_position(position))
            })
        })?;
        let summary = path.motion();
        let path_id = path.id();
        self.published = project(local, movement, parent, movement.context().transport);
        if summary.flags & 0xa00 == 0 && movement.flags() as u32 & 0x4220_0000 == 0 {
            self.apply_spline_step(
                SplineStep {
                    current,
                    local,
                    movement,
                    summary,
                    duration,
                    scene_collision: self.scene_collision,
                },
                dimensions,
                geometry,
            )?;
        } else {
            // Spatial/hover splines supply their own position, including the arc.
            self.position = local.position();
            self.orientation = local.orientation();
            self.flags = movement.flags() as u32 & 0x77ff_fdff;
            self.secondary = (movement.flags() >> 32) as u16;
            self.context = movement.context();
            if summary.flags & 0x400 != 0 {
                self.stop_path_fall()?;
                self.reanchor()?;
            }
        }
        if summary.flags & 0x400 != 0 {
            self.input_refresh_pending = true;
            if self.active {
                self.input_axis_reset_pending = true;
                let previous_ms = self.time_ms;
                self.time_ms = next_ms;
                let movement = self.snapshot_message(WorldMovementKind::Heartbeat)?;
                output.push_back(PlayerMovementOutput::SplineDone { movement, path_id });
                self.heartbeat_ms = next_ms.wrapping_add(500);
                self.time_ms = previous_ms;
            }
        }
        Ok(())
    }

    pub(super) fn refresh_path_input(
        &mut self,
        input: &mut PlayerInputState,
        world: &ActiveWorld,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        if !std::mem::take(&mut self.input_refresh_pending) || !self.active {
            return Ok(());
        }
        if std::mem::take(&mut self.input_axis_reset_pending) {
            input.clear_active_axes();
        }
        let admission = self.admission(world);
        let mut effects = Vec::with_capacity(5);
        input.resolve(admission, &mut |effect| effects.push(effect));
        for effect in effects {
            self.apply(effect, admission, world, output)?;
        }
        Ok(())
    }
}
