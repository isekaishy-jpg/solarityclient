//! Ordinary remote timeline driving the shared native ground/fall integrator.

use std::collections::VecDeque;

use solarity_ecs::{
    WorldMovementContext, WorldMovementFall, WorldMovementState, WorldMovementTransport,
    WorldObjectIdentity, WorldTransform,
};
use solarity_network::{RemoteMovement, WorldMovementKind};
use solarity_systems::{
    MovementFallState, MovementGroundProfile, MovementSpline, MovementTransportChange,
    RemoteMovementBlend, RemoteMovementClock, RemoteMovementPose, RemoteMovementReceipt,
};

use super::super::passenger::PassengerParent;
use super::super::{
    LocalMovement, LocalMovementGeometry, MovementPhase, RuntimePlayerMovementError,
};
use super::inbox::RemoteMovementInput;
use crate::application::unit_animation::{
    UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
};

#[cfg(test)]
#[path = "../../../../tests/application/remote_movement_state.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/application/remote_passenger.rs"]
mod passenger_tests;

#[cfg(test)]
#[path = "../../../../tests/application/remote_spline_ground.rs"]
mod spline_ground_tests;

/// A stable, wrapping-clock queue entry after native delay admission.
struct Scheduled {
    message: RemoteMovement,
    time_ms: u32,
}

/// Native normal queue playback and path replacement have different actions.
#[derive(Clone, Copy, PartialEq)]
enum SnapshotApplication {
    Immediate,
    Queued,
    Flush,
}

/// Predicted state tied to one world/entity/GUID lifetime.
pub(super) struct RemoteUnit {
    pub identity: WorldObjectIdentity,
    pub published: (WorldTransform, WorldMovementState),
    pub time_ms: u32,
    pub motion: Option<LocalMovement>,
    pub path: Option<MovementSpline>,
    /// Retained lifetime/frame while a spline or packet-only mode owns the local pose.
    pub path_parent: Option<PassengerParent>,
    /// Local collision normal retained when packet/path input replaces motion.
    pub ground_normal: glam::Vec3,
    /// Previous scene traversal's unit callback bit, consumed once per service pass.
    pub scene_collision: bool,
    pub animation_events: VecDeque<UnitMovementAnimationEvent>,
    /// Unit +0x784 survives packet and spline replacement within this lifetime.
    pub previous_water_depth: f32,
    clock: RemoteMovementClock,
    commands: VecDeque<Scheduled>,
}

impl RemoteUnit {
    /// Seeds the timeline without discarding the first packet's receipt order.
    pub fn new(
        identity: WorldObjectIdentity,
        transform: WorldTransform,
        movement: WorldMovementState,
        time_ms: u32,
    ) -> Self {
        Self {
            identity,
            published: (transform, movement),
            time_ms,
            motion: None,
            path: None,
            path_parent: None,
            ground_normal: glam::Vec3::Z,
            scene_collision: false,
            animation_events: VecDeque::new(),
            previous_water_depth: 0.0,
            clock: RemoteMovementClock::default(),
            commands: VecDeque::new(),
        }
    }

    /// Applies all receipt-ordered inputs before the ordinary capped frame.
    /// Immediate corrections catch up before the following packet is admitted.
    pub fn process<G: LocalMovementGeometry>(
        &mut self,
        events: impl IntoIterator<Item = RemoteMovementInput>,
        now_ms: u32,
        dimensions: [f32; 3],
        profile: MovementGroundProfile,
        geometry: &mut G,
        target_position: impl Fn(u64) -> Option<glam::Vec3>,
    ) -> Result<(), RuntimePlayerMovementError> {
        for event in events {
            match event {
                RemoteMovementInput::Baseline {
                    transform,
                    movement,
                    spline,
                    receipt_ms,
                } => {
                    self.baseline(transform, movement, receipt_ms);
                    self.path = spline.map(|path| *path);
                }
                RemoteMovementInput::Command {
                    message,
                    receipt_ms,
                } => {
                    self.initialize_motion(geometry)?;
                    if self.receive(message, receipt_ms, now_ms, geometry)? {
                        // 006EA550 catches up an immediate snapshot in full
                        // 250 ms chunks; the ordinary frame cap is separate.
                        while (now_ms.wrapping_sub(self.time_ms) as i32) > 250 {
                            self.advance(
                                self.time_ms.wrapping_add(250),
                                dimensions,
                                profile,
                                geometry,
                                &target_position,
                            )?;
                        }
                        self.advance(now_ms, dimensions, profile, geometry, &target_position)?;
                    }
                }
                RemoteMovementInput::Path {
                    message,
                    receipt_ms,
                    stop_distance_tolerance,
                } => {
                    self.receive_path(
                        &message,
                        receipt_ms,
                        stop_distance_tolerance,
                        geometry,
                        &target_position,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Replaces authoritative motion while preserving the lifetime's clock history.
    pub fn baseline(
        &mut self,
        transform: WorldTransform,
        movement: WorldMovementState,
        time_ms: u32,
    ) {
        if let Some(motion) = &mut self.motion {
            self.animation_events.append(&mut motion.animation_events);
            self.ground_normal = motion.ground_normal;
        }
        self.published = (transform, movement);
        self.time_ms = time_ms;
        self.motion = None;
        self.path = None;
        self.path_parent = None;
        self.commands.clear();
    }

    pub fn snapshot(&self) -> (WorldTransform, WorldMovementState) {
        let (transform, mut movement) = self
            .motion
            .as_ref()
            .map_or(self.published, LocalMovement::snapshot);
        if let Some(path) = &self.path {
            movement = movement
                .with_flags(movement.flags() | (self.published.1.flags() & 0x0800_0000))
                .with_spline(path.motion());
        }
        (transform, movement)
    }

    /// Constructs the shared ground/fall engine in the admitted parent's coordinates.
    pub fn initialize_motion<G: LocalMovementGeometry>(
        &mut self,
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        if self.path.is_some() || self.motion.is_none() {
            self.refresh_path_parent(geometry)?;
        }
        let (transform, movement) = self.published;
        let walking_path = self.path.as_ref().is_some_and(|path| {
            path.motion().flags & 0xa00 == 0 && movement.flags() as u32 & 0x4220_0000 == 0
        });
        // Swimming shares the local 3D owner; flight retains its separate path.
        if self.motion.is_none()
            && (self.path.is_none() || walking_path)
            && movement.flags() as u32
                & (if walking_path {
                    0x4200_0000
                } else {
                    0x4a00_0000
                })
                == 0
            && (movement.flags() as u32 & 0x200000 != 0 || movement.flags() as u32 & 0xc000c0 == 0)
        {
            let mut context = movement.context();
            let transport = context.transport.filter(|parent| parent.guid != 0);
            let parent = match transport {
                Some(transport) => geometry.passenger(transport.guid)?,
                None => None,
            };
            let mut motion = if let (Some(parent), Some(transport)) = (parent, transport) {
                LocalMovement::new_on_parent(
                    self.identity,
                    transform,
                    movement,
                    self.time_ms,
                    parent,
                    transport,
                )?
            } else {
                // 988920 stores the world pose first. 9872C0 returns GUID zero
                // when the transmitted parent cannot be resolved.
                context.transport = None;
                LocalMovement::new(
                    self.identity,
                    transform,
                    WorldMovementState::new(
                        movement.flags() & !(0x400_u64 << 32),
                        movement.speeds(),
                        context,
                    ),
                    self.time_ms,
                )?
            };
            motion.remote = true;
            motion.ground_normal = self.ground_normal;
            motion.context.transport = transport.filter(|_| parent.is_some());
            self.motion = Some(motion);
            if self.path.is_none() {
                self.path_parent = None;
            }
        }
        if let Some(motion) = &mut self.motion {
            motion.refresh_passenger(geometry)?;
        }
        Ok(())
    }

    /// Applies immediate corrections or inserts a future command in native order.
    pub fn receive<G: LocalMovementGeometry>(
        &mut self,
        message: RemoteMovement,
        receipt_ms: u32,
        frame_ms: u32,
        geometry: &mut G,
    ) -> Result<bool, RuntimePlayerMovementError> {
        let current = self.snapshot();
        let admission = self.clock.admit(RemoteMovementReceipt {
            server_ms: message.context.timestamp_ms,
            receipt_ms,
            frame_ms,
            flags: current.1.flags() as u32,
            has_pending_commands: !self.commands.is_empty(),
            has_path: current.1.spline().is_some(),
        });
        if admission.immediate {
            // 006EB4E0 discards ordinary queued events after an immediate
            // authoritative correction (only native event 49 survives).
            self.commands.clear();
            self.apply(
                message,
                admission.timeline_ms,
                SnapshotApplication::Immediate,
                geometry,
            )?;
        } else {
            let was_empty = self.commands.is_empty();
            let index = self
                .commands
                .iter()
                .position(|command| {
                    (admission.timeline_ms.wrapping_sub(command.time_ms) as i32) < 0
                })
                .unwrap_or(self.commands.len());
            self.commands.insert(
                index,
                Scheduled {
                    message,
                    time_ms: admission.timeline_ms,
                },
            );
            if was_empty {
                self.seed_blend(frame_ms);
            } else if index == 0 {
                // Native insertion keeps the retained deltas/duration, while
                // sampling reads the newly first command's endpoint.
                if let (Some(motion), Some(next)) = (&mut self.motion, self.commands.front())
                    && let Some(blend) = &mut motion.blend
                {
                    blend.retarget(pose(next.message), next.time_ms);
                }
            }
        }
        Ok(admission.immediate)
    }

    /// Reanchors the authoritative snapshot with the correct launch/flag policy.
    fn apply<G: LocalMovementGeometry>(
        &mut self,
        message: RemoteMovement,
        time_ms: u32,
        application: SnapshotApplication,
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        let queued = application != SnapshotApplication::Immediate;
        let previous = self.snapshot().1;
        let mut context = context(message);
        let next_parent = match context.transport {
            Some(parent) => geometry.passenger(parent.guid)?,
            None => None,
        };
        // Queued snapshots retain the full launch basis. 6EA1D0 rotates it
        // through the old and new parent frames when their GUIDs differ.
        let retained_direction = if queued {
            self.motion.as_ref().map(|motion| {
                let mut direction = match motion.phase {
                    MovementPhase::Swimming(trajectory) => trajectory.direction(),
                    MovementPhase::Fall(fall) => fall.snapshot().direction,
                    MovementPhase::Ground { .. } => motion.ground.travel_direction(),
                };
                if motion.passenger.map(|parent| parent.identity)
                    != next_parent.map(|parent| parent.identity)
                {
                    if let Some(parent) = motion.passenger {
                        direction = parent.frame.exit_change().direction(direction);
                    }
                    if let Some(parent) = next_parent {
                        direction = parent.frame.entry_change().direction(direction);
                    }
                }
                direction
            })
        } else {
            None
        };
        // 6EA9B0 asks 6EA1D0 to change links before copying its local pose.
        // A failed new admission has already unlinked the old parent; unlike
        // 9872C0's immediate path, it does not then copy the packet's world pose.
        if queued && context.transport.is_some() && next_parent.is_none() {
            if let Some(motion) = &mut self.motion {
                if motion.passenger.is_some() {
                    motion.flags &= !0x0800_0000;
                }
                motion.passenger = None;
                motion.passenger_seat = context.transport.map_or(-1, |parent| parent.seat);
                if let (MovementPhase::Fall(fall), Some(direction)) =
                    (motion.phase, retained_direction)
                {
                    let mut fall = fall.snapshot();
                    fall.direction = direction;
                    fall.horizontal_direction =
                        MovementTransportChange::horizontal_direction(direction);
                    motion.phase = MovementPhase::Fall(MovementFallState::new(fall)?);
                }
                motion.refresh_passenger(geometry)?;
            }
            return Ok(());
        }
        // 006EA9B0 changes the fall clock/height but keeps the launch bases
        // and secondary flags. Event 10 first runs 009883F0's jump admission.
        if queued
            && context.falling.is_some()
            && let Some(motion) = &self.motion
        {
            let retained = previous.context().falling.unwrap_or(WorldMovementFall {
                vertical_speed: motion.retained_downward_speed,
                direction_sin: motion.ground.direction().y,
                direction_cos: motion.ground.direction().x,
                horizontal_speed: motion.ground.speed(),
            });
            context.falling = Some(
                if application == SnapshotApplication::Queued
                    && message.kind == WorldMovementKind::Jump
                    && motion.flags & 0x4200_1a00 == 0
                    && (motion.secondary & 4 != 0 || motion.flags & 0x400 == 0)
                    && motion.secondary & 2 == 0
                {
                    WorldMovementFall {
                        vertical_speed: f32::from_bits(0xc0fe_93d8),
                        direction_sin: motion.ground.direction().y,
                        direction_cos: motion.ground.direction().x,
                        horizontal_speed: motion.ground.speed(),
                    }
                } else {
                    retained
                },
            );
            if let (Some(fall), Some(direction)) = (&mut context.falling, retained_direction) {
                let horizontal = MovementTransportChange::horizontal_direction(direction);
                fall.direction_cos = horizontal.x;
                fall.direction_sin = horizontal.y;
            }
        }
        let secondary = if queued {
            previous.flags()
        } else {
            message.flags
        } & 0xffff_0000_0000;
        let mut flags =
            (previous.flags() & 0x8800_0000) | (message.flags & 0x77ff_fdff) | secondary;
        if context.transport.is_some() {
            flags |= 0x200;
        }
        if application != SnapshotApplication::Flush && message.flags & 0x0800_0000 == 0 {
            flags &= !0x0080_0800_0000;
            self.path = None;
        }
        let mut movement = WorldMovementState::new(flags, previous.speeds(), context);
        if (message.flags & 0x0800_0000 != 0 || application == SnapshotApplication::Flush)
            && let Some(spline) = previous.spline()
        {
            movement = movement.with_spline(spline);
        }
        self.published = (
            WorldTransform::new(
                glam::Vec3::from_array(message.position),
                message.orientation,
            ),
            movement,
        );
        if let Some(motion) = &mut self.motion {
            self.animation_events.append(&mut motion.animation_events);
            self.ground_normal = motion.ground_normal;
        }
        self.motion = None;
        self.path_parent = None;
        self.time_ms = time_ms;
        self.initialize_motion(geometry)?;
        if let (Some(motion), Some(direction)) = (&mut self.motion, retained_direction)
            && let MovementPhase::Fall(fall) = motion.phase
        {
            let mut fall = fall.snapshot();
            fall.direction = direction;
            motion.phase = MovementPhase::Fall(MovementFallState::new(fall)?);
        }
        if application != SnapshotApplication::Flush
            && let Some(motion) = &mut self.motion
        {
            let kind = match message.kind {
                _ if previous.flags() & 0x1000 != 0 && movement.flags() & 0x1000 == 0 => {
                    Some(UnitMovementAnimationEventKind::Land {
                        previous_flags: previous.flags() as u32,
                        forced: previous
                            .context()
                            .falling
                            .is_some_and(|fall| fall.vertical_speed != 0.0),
                        slow: motion.ground.speed() <= motion.speeds.walk() * 2.0,
                    })
                }
                WorldMovementKind::Jump => Some(UnitMovementAnimationEventKind::Jump),
                WorldMovementKind::Heartbeat
                | WorldMovementKind::SetFacing
                | WorldMovementKind::SetPitch
                | WorldMovementKind::StartPitchUp
                | WorldMovementKind::StartPitchDown
                | WorldMovementKind::StopPitch => None,
                WorldMovementKind::FallLand => None,
                _ => Some(UnitMovementAnimationEventKind::Changed),
            };
            if let Some(kind) = kind {
                motion.notify_animation(kind);
            }
        }
        Ok(())
    }

    /// 006ED7E0 -> 006ED0F0 applies every retained snapshot, including future
    /// timestamps, without replaying jump/axis actions or advancing physics.
    pub fn flush<G: LocalMovementGeometry>(
        &mut self,
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        while let Some(command) = self.commands.pop_front() {
            self.apply(
                command.message,
                self.time_ms,
                SnapshotApplication::Flush,
                geometry,
            )?;
        }
        if let Some(motion) = &mut self.motion {
            motion.blend = None;
        }
        Ok(())
    }

    /// Captures 006EA6A0's deltas to the next command after an applied event.
    fn seed_blend(&mut self, now_ms: u32) {
        let Some(motion) = &mut self.motion else {
            return;
        };
        motion.blend = self.commands.front().and_then(|next| {
            RemoteMovementBlend::new(
                RemoteMovementPose {
                    transform: WorldTransform::new(motion.position, motion.orientation),
                    pitch: motion.context.pitch_radians.unwrap_or(0.0),
                    transport_guid: motion.passenger.map_or(0, |parent| parent.identity.guid()),
                },
                pose(next.message),
                now_ms,
                next.time_ms,
                motion.flags,
            )
        });
    }

    /// Advances at most 250 ms, splitting simulation at due command timestamps.
    pub fn advance<G: LocalMovementGeometry>(
        &mut self,
        end_ms: u32,
        dimensions: [f32; 3],
        profile: MovementGroundProfile,
        geometry: &mut G,
        target_position: impl Fn(u64) -> Option<glam::Vec3>,
    ) -> Result<(), RuntimePlayerMovementError> {
        self.initialize_motion(geometry)?;
        let delta = end_ms.wrapping_sub(self.time_ms) as i32;
        if delta <= 0 {
            return Ok(());
        }
        if delta > 250 {
            self.time_ms = end_ms.wrapping_sub(250);
            if let Some(motion) = &mut self.motion {
                motion.time_ms = self.time_ms;
            }
        }
        loop {
            let command_ms = self.commands.front().map(|next| {
                if (next.time_ms.wrapping_sub(self.time_ms) as i32) < 0 {
                    self.time_ms
                } else {
                    next.time_ms
                }
            });
            let next_ms = command_ms
                .filter(|next| (end_ms.wrapping_sub(*next) as i32) >= 0)
                .unwrap_or(end_ms);
            let duration = next_ms.wrapping_sub(self.time_ms);
            let (transform, movement) = self.snapshot();
            // 6EAC40 checks the movement owner's coordinate lane, which is
            // parent-local for passengers, before evaluating its interval.
            let position = self.motion.as_ref().map_or_else(
                || {
                    movement
                        .context()
                        .transport
                        .map_or(transform.position(), |parent| parent.position)
                },
                |motion| motion.position,
            );
            let can_advance = movement.flags() & 0x40c0_10ff != 0 && in_map_bounds(position);
            let path_interval = self
                .path
                .as_ref()
                .is_some_and(|path| path.motion().flags & 0x400 == 0);
            if can_advance && path_interval {
                self.advance_path(next_ms, dimensions, profile, geometry, &target_position)?;
            } else if let Some(motion) = &mut self.motion {
                motion.remote_profile = Some(profile);
                if can_advance {
                    motion.interval(duration, dimensions, geometry, &mut VecDeque::new())?;
                }
            }
            if let Some(motion) = &mut self.motion {
                motion.time_ms = next_ms;
            }
            self.time_ms = next_ms;
            if command_ms != Some(next_ms) {
                break;
            }
            let Some(command) = self.commands.pop_front() else {
                break;
            };
            if blocked_command(
                self.snapshot().1.flags() as u32 & !0x200,
                command.message.kind,
            ) {
                if let Some(motion) = &mut self.motion {
                    match (&mut motion.blend, self.commands.front()) {
                        (Some(blend), Some(next)) => {
                            blend.retarget(pose(next.message), next.time_ms)
                        }
                        (_, None) => motion.blend = None,
                        (None, Some(_)) => {}
                    }
                }
                continue;
            }
            self.apply(
                command.message,
                next_ms,
                SnapshotApplication::Queued,
                geometry,
            )?;
            self.seed_blend(next_ms);
        }
        self.published = self.snapshot();
        Ok(())
    }
}

/// 006EAC40 calls 00406DE0 with a two-yard map-edge margin. Z must be finite
/// but is unbounded; the half-width is the executable's 009E2ACC float.
fn in_map_bounds(position: glam::Vec3) -> bool {
    let edge = f64::from(f32::from_bits(0x4685_5555)) - 2.0;
    position.is_finite() && f64::from(position.x).abs() < edge && f64::from(position.y).abs() < edge
}

/// 006EF860 discards these queued commands, including their snapshots, while
/// internally immobilized, rooted, or waiting to restore a root after path completion.
/// The caller removes the wire transport bit; native internal 0x200 is unrelated.
fn blocked_command(flags: u32, kind: WorldMovementKind) -> bool {
    match kind {
        WorldMovementKind::StartForward
        | WorldMovementKind::StartBackward
        | WorldMovementKind::Stop
        | WorldMovementKind::StartStrafeLeft
        | WorldMovementKind::StartStrafeRight
        | WorldMovementKind::StopStrafe
        | WorldMovementKind::Jump => flags & 0x100a00 != 0,
        WorldMovementKind::StartSwim | WorldMovementKind::StopSwim => flags & 0xa00 != 0,
        _ => false,
    }
}

/// Supplies the next snapshot's position and angles to native interpolation.
fn pose(message: RemoteMovement) -> RemoteMovementPose {
    let transform = message
        .context
        .transport
        .filter(|parent| parent.guid != 0)
        .map_or_else(
            || {
                WorldTransform::new(
                    glam::Vec3::from_array(message.position),
                    message.orientation,
                )
            },
            |parent| {
                WorldTransform::new(glam::Vec3::from_array(parent.position), parent.orientation)
            },
        );
    RemoteMovementPose {
        transform,
        pitch: message.context.pitch_radians.unwrap_or(0.0),
        transport_guid: message.context.transport.map_or(0, |parent| parent.guid),
    }
}

/// Retains conditional fields; passenger points remain explicitly parent-local.
fn context(message: RemoteMovement) -> WorldMovementContext {
    WorldMovementContext {
        timestamp_ms: message.context.timestamp_ms,
        transport: message
            .context
            .transport
            .filter(|parent| parent.guid != 0)
            .map(|parent| WorldMovementTransport {
                guid: parent.guid,
                position: glam::Vec3::from_array(parent.position),
                orientation: parent.orientation,
                time_ms: parent.time_ms,
                seat: parent.seat,
                interpolated_time_ms: parent.interpolated_time_ms,
            }),
        pitch_radians: message.context.pitch_radians,
        fall_time_ms: message.context.fall_time_ms,
        falling: message.context.falling.map(|fall| WorldMovementFall {
            vertical_speed: fall.vertical_speed,
            direction_sin: fall.direction_sin,
            direction_cos: fall.direction_cos,
            horizontal_speed: fall.horizontal_speed,
        }),
        spline_elevation: message.context.spline_elevation,
    }
}
