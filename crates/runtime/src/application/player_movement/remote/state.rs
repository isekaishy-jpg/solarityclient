//! Ordinary remote timeline driving the shared native ground/fall integrator.

use std::collections::VecDeque;

use solarity_ecs::{
    WorldMovementContext, WorldMovementFall, WorldMovementState, WorldObjectIdentity,
    WorldTransform,
};
use solarity_network::{RemoteMovement, WorldMovementKind};
use solarity_systems::{
    MovementGroundProfile, MovementSpline, RemoteMovementBlend, RemoteMovementClock,
    RemoteMovementPose, RemoteMovementReceipt,
};

use super::super::{LocalMovement, LocalMovementGeometry, RuntimePlayerMovementError};
use super::inbox::RemoteMovementInput;
use crate::application::unit_animation::{
    UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
};

#[cfg(test)]
#[path = "../../../../tests/application/remote_movement_state.rs"]
mod tests;

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
    pub animation_events: VecDeque<UnitMovementAnimationEvent>,
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
            animation_events: VecDeque::new(),
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
                    self.initialize_motion()?;
                    if self.receive(message, receipt_ms, now_ms)? {
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
                    self.flush()?;
                    // 0073C8E0 uses the current placement after 006ED7E0;
                    // no path sample may overwrite a just-flushed snapshot.
                    let (current, movement) = self.snapshot();
                    let prepared = crate::application::gameplay_session::prepare_monster_move(
                        current,
                        movement,
                        &message,
                        receipt_ms,
                        stop_distance_tolerance,
                        &target_position,
                    )?;
                    self.baseline(prepared.transform, prepared.movement, receipt_ms);
                    self.path = prepared.spline;
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
        }
        self.published = (transform, movement);
        self.time_ms = time_ms;
        self.motion = None;
        self.path = None;
        self.commands.clear();
    }

    pub fn snapshot(&self) -> (WorldTransform, WorldMovementState) {
        self.motion
            .as_ref()
            .map_or(self.published, LocalMovement::snapshot)
    }

    /// Constructs the shared ground/fall engine when no path or parent owns travel.
    pub fn initialize_motion(&mut self) -> Result<(), RuntimePlayerMovementError> {
        let (transform, movement) = self.published;
        // Liquid, flight, passenger, and pitch-arc travel require the separate
        // three-dimensional trajectory owner. 00987950 integrates a pitch arc
        // even with a ground launch basis; the horizontal solver cannot stand in.
        if self.motion.is_none()
            && self.path.is_none()
            && movement.transport_guid().is_none()
            && movement.flags() as u32 & 0x4ae0_00c0 == 0
        {
            let mut motion = LocalMovement::new(self.identity, transform, movement, self.time_ms)?;
            motion.remote = true;
            self.motion = Some(motion);
        }
        Ok(())
    }

    /// Applies immediate corrections or inserts a future command in native order.
    pub fn receive(
        &mut self,
        message: RemoteMovement,
        receipt_ms: u32,
        frame_ms: u32,
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
    fn apply(
        &mut self,
        message: RemoteMovement,
        time_ms: u32,
        application: SnapshotApplication,
    ) -> Result<(), RuntimePlayerMovementError> {
        let queued = application != SnapshotApplication::Immediate;
        let previous = self.snapshot().1;
        let mut context = context(message);
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
        }
        let secondary = if queued {
            previous.flags()
        } else {
            message.flags
        } & 0xffff_0000_0000;
        let mut flags =
            (previous.flags() & 0x8800_0200) | (message.flags & 0x77ff_fdff) | secondary;
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
        }
        self.motion = None;
        self.time_ms = time_ms;
        self.initialize_motion()?;
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
    pub fn flush(&mut self) -> Result<(), RuntimePlayerMovementError> {
        while let Some(command) = self.commands.pop_front() {
            self.apply(command.message, self.time_ms, SnapshotApplication::Flush)?;
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
                    transport_guid: 0,
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
        self.initialize_motion()?;
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
            let can_advance =
                movement.flags() & 0x40c0_10ff != 0 && in_map_bounds(transform.position());
            if can_advance && let Some(path) = &mut self.path {
                self.published = path.advance_movement(
                    next_ms,
                    self.published.1,
                    self.published.0,
                    &target_position,
                )?;
            }
            if let Some(motion) = &mut self.motion {
                motion.remote_profile = Some(profile);
                if can_advance {
                    motion.interval(duration, dimensions, geometry, &mut VecDeque::new())?;
                }
                motion.time_ms = next_ms;
            }
            self.time_ms = next_ms;
            if command_ms != Some(next_ms) {
                break;
            }
            let Some(command) = self.commands.pop_front() else {
                break;
            };
            if blocked_command(self.snapshot().1.flags() as u32, command.message.kind) {
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
            self.apply(command.message, next_ms, SnapshotApplication::Queued)?;
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
/// attached, rooted, or waiting to restore a root after path completion.
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
    RemoteMovementPose {
        transform: WorldTransform::new(
            glam::Vec3::from_array(message.position),
            message.orientation,
        ),
        pitch: message.context.pitch_radians.unwrap_or(0.0),
        transport_guid: message.context.transport.map_or(0, |parent| parent.guid),
    }
}

/// Converts supported world-space conditional fields without changing wire clocks.
fn context(message: RemoteMovement) -> WorldMovementContext {
    WorldMovementContext {
        timestamp_ms: message.context.timestamp_ms,
        transport: None,
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
