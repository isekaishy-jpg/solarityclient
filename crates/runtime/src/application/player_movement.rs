//! Timestamped local movement, collision continuation, and frozen notifications.

mod passenger;
pub(super) mod remote;

use passenger::{PassengerClock, PassengerParent};

use std::collections::VecDeque;

use super::player_camera::{
    PlayerCameraFollowSettings, PlayerCameraInput, PlayerCameraMouseSettings,
    PlayerCameraZoomSettings,
};
use super::player_control::PlayerControlEvent;
use super::unit_animation::{UnitMovementAnimationEvent, UnitMovementAnimationEventKind};
use glam::{Vec2, Vec3};
use solarity_ecs::{
    ActiveWorld, WorldMovementContext, WorldMovementFall, WorldMovementSpeeds, WorldMovementState,
    WorldObjectIdentity, WorldTransform,
};
use solarity_network::{
    ObjectMovementContext, ObjectMovementFall, WorldMovementKind, WorldMovementMessage,
};
use solarity_systems::{
    MovementBspCacheMode, MovementFallAdmission, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallMode, MovementFallPhase,
    MovementFallSnapshot, MovementFallState, MovementFallTrajectory, MovementGroundContinuation,
    MovementGroundInterval, MovementGroundProfile, MovementGroundSnapshot, MovementGroundState,
    MovementGroundTrajectory, MovementIntervalMode, MovementIntervalRequest,
    MovementSupportProfile,
};
use solarity_ui::UiMovementCommand;
use thiserror::Error;

use super::{
    RuntimeGameObjectPresentation, RuntimeGameplayCoordinator, RuntimeMovementGeometry,
    RuntimeMovementQuery, RuntimeStaticMovementResidency, RuntimeTerrainCoordinator,
};
use crate::input::{PlayerInputAdmission, PlayerInputEffect, PlayerInputState};

/// A local movement interval or notification failed admission.
#[derive(Debug, Error)]
pub enum RuntimePlayerMovementError {
    /// A remote server path failed geometry or timeline admission.
    #[error(transparent)]
    Spline(#[from] solarity_systems::MovementSplineError),
    /// Model dimensions could not be resolved through the stock catalogs.
    #[error(transparent)]
    Appearance(#[from] solarity_systems::UnitModelAppearanceError),
    /// A native ground trajectory rejected owner state.
    #[error(transparent)]
    Trajectory(#[from] solarity_systems::MovementGroundTrajectoryError),
    /// A native yaw trajectory rejected owner state.
    #[error(transparent)]
    Yaw(#[from] solarity_systems::MovementYawTrajectoryError),
    /// Ground contact failed arithmetic or geometry admission.
    #[error(transparent)]
    Ground(#[from] solarity_systems::MovementGroundAdvanceError),
    /// Airborne contact failed arithmetic or geometry admission.
    #[error(transparent)]
    Fall(#[from] solarity_systems::MovementFallAdvanceError),
    /// The fall curve cannot represent the supplied state.
    #[error(transparent)]
    FallCurve(#[from] solarity_systems::MovementFallError),
    /// Resident geometry contains an invalid reference or volume.
    #[error(transparent)]
    Geometry(#[from] super::RuntimeStaticMovementError),
    /// The generated wire snapshot is inconsistent.
    #[error(transparent)]
    Packet(#[from] solarity_network::WorldMovementEncodeError),
    /// World ownership changed or its writer failed.
    #[error(transparent)]
    Gameplay(#[from] super::RuntimeGameplayError),
    /// Required controlled-player state is absent.
    #[error(transparent)]
    World(#[from] solarity_ecs::WorldStateError),
    /// A resident passenger parent has an invalid placement.
    #[error(transparent)]
    PassengerPlacement(#[from] solarity_systems::GameObjectPlacementError),
    /// This mode still requires its native movement response owner.
    #[error("local movement mode is not implemented: flags={flags:#x}")]
    UnsupportedMode {
        /// Exact admitted native movement flags.
        flags: u32,
    },
}

#[derive(Clone, Copy)]
pub(super) enum PlayerMovementOutput {
    Movement(WorldMovementMessage),
    SkippedTime {
        guid: u64,
        milliseconds: u32,
    },
    StandState(u32),
    ActiveMover(u64),
    AreaTrigger {
        heartbeat: WorldMovementMessage,
        trigger_id: u32,
    },
}

#[derive(Clone, Copy)]
enum MovementCommand {
    Input(UiMovementCommand),
    Control(PlayerControlEvent),
    /// Native event 9 is queued when a transport destroys its passenger link.
    SupportRecheck {
        timestamp_ms: u32,
    },
    MouseMotion {
        delta: [f32; 2],
        settings: PlayerCameraMouseSettings,
        timestamp_ms: u32,
    },
}

impl MovementCommand {
    fn timestamp_ms(self) -> u32 {
        match self {
            Self::MouseMotion { timestamp_ms, .. } | Self::SupportRecheck { timestamp_ms } => {
                timestamp_ms
            }
            Self::Input(command) => command.timestamp_ms,
            Self::Control(
                PlayerControlEvent::StandState { timestamp_ms, .. }
                | PlayerControlEvent::PlayerControl { timestamp_ms, .. }
                | PlayerControlEvent::ActiveMover { timestamp_ms, .. },
            ) => timestamp_ms,
        }
    }
}

#[derive(Default)]
pub(super) struct RuntimePlayerMovement {
    camera_zoom_settings: PlayerCameraZoomSettings,
    camera_follow_settings: PlayerCameraFollowSettings,
    camera_cvar_revision: Option<u64>,
    owner: Option<LocalMovement>,
    input: PlayerInputState,
    commands: VecDeque<MovementCommand>,
    output: VecDeque<PlayerMovementOutput>,
    geometry: RuntimeMovementQuery,
}

struct LocalMovement {
    passenger: Option<PassengerParent>,
    passenger_seat: i8,
    passenger_clock: PassengerClock,
    remote: bool,
    remote_profile: Option<MovementGroundProfile>,
    blend: Option<solarity_systems::RemoteMovementBlend>,
    animation_events: VecDeque<UnitMovementAnimationEvent>,
    camera: PlayerCameraInput,
    initial_contact_pending: bool,
    active: bool,
    client_control: bool,
    stand_state: u8,
    identity: WorldObjectIdentity,
    position: Vec3,
    orientation: f32,
    flags: u32,
    secondary: u16,
    speeds: WorldMovementSpeeds,
    context: WorldMovementContext,
    retained_launch_height: f32,
    retained_downward_speed: f32,
    anchor: Vec3,
    elapsed_ms: u32,
    time_ms: u32,
    heartbeat_ms: u32,
    ground: MovementGroundTrajectory,
    yaw: solarity_systems::MovementYawTrajectory,
    phase: MovementPhase,
    published: (WorldTransform, WorldMovementState),
}

#[derive(Clone, Copy)]
enum MovementPhase {
    Ground { step_anchor: Option<f32> },
    Fall(MovementFallState),
}

trait LocalMovementGeometry: solarity_systems::MovementGeometry {
    /// Static geometry reports zero; dynamic collectors preserve the native owner GUID.
    fn contact_guid(&self, _contact: Option<Self::TriangleIdentity>) -> u64 {
        0
    }

    /// Resolves a parent through the currently borrowed world generation.
    fn passenger(&self, _guid: u64) -> Result<Option<PassengerParent>, RuntimePlayerMovementError> {
        Ok(None)
    }

    /// Virtual +0xF0 may retain an old parent after a zero-GUID contact.
    fn retains_passenger(
        &self,
        _identity: WorldObjectIdentity,
        _position: Vec3,
    ) -> Result<bool, RuntimePlayerMovementError> {
        Ok(false)
    }

    /// A coordinate change invalidates every cached world face and bound.
    fn set_passenger_frame(&mut self, _frame: Option<solarity_systems::MovementTransportFrame>) {}

    fn check_failure(&mut self) -> Result<(), RuntimePlayerMovementError> {
        Ok(())
    }

    fn collect(
        &mut self,
        request: MovementIntervalRequest,
    ) -> Result<bool, RuntimePlayerMovementError>;
}

impl LocalMovementGeometry for RuntimeMovementGeometry<'_> {
    fn contact_guid(&self, contact: Option<Self::TriangleIdentity>) -> u64 {
        use super::RuntimeMovementOwner;
        match contact {
            Some(RuntimeMovementOwner::GameObject { reported_guid, .. }) => reported_guid,
            Some(
                RuntimeMovementOwner::GameObjectWorldModel { identity }
                | RuntimeMovementOwner::GameObjectMapModel { identity },
            ) => identity.guid(),
            Some(
                RuntimeMovementOwner::Static(_)
                | RuntimeMovementOwner::GameObjectWorldModelDoodad { .. },
            )
            | None => 0,
        }
    }

    fn passenger(&self, guid: u64) -> Result<Option<PassengerParent>, RuntimePlayerMovementError> {
        PassengerParent::resolve(self, guid)
    }

    fn retains_passenger(
        &self,
        identity: WorldObjectIdentity,
        position: Vec3,
    ) -> Result<bool, RuntimePlayerMovementError> {
        Ok(
            RuntimeMovementGeometry::retains_passenger(self, identity, position)
                .map_err(super::RuntimeStaticMovementError::from)?,
        )
    }

    fn set_passenger_frame(&mut self, frame: Option<solarity_systems::MovementTransportFrame>) {
        self.set_transport_frame(frame);
    }

    fn check_failure(&mut self) -> Result<(), RuntimePlayerMovementError> {
        if let Some(super::RuntimeMovementGeometryFailure::Invalid(error)) = self.take_failure() {
            return Err(error.into());
        }
        Ok(())
    }

    fn collect(
        &mut self,
        request: MovementIntervalRequest,
    ) -> Result<bool, RuntimePlayerMovementError> {
        Ok(self.collect_interval(request)? == RuntimeStaticMovementResidency::Ready)
    }
}

impl RuntimePlayerMovement {
    /// Samples the live camera after this frame's collision feedback.
    pub(super) fn camera_view(&self) -> Option<solarity_ecs::PlayerViewState> {
        self.owner
            .as_ref()
            .map(|owner| owner.camera.view(owner.world_orientation()))
    }

    /// Orders an entry notification after all earlier local movement writes.
    pub(super) fn queue_area_trigger(&mut self, heartbeat: WorldMovementMessage, trigger_id: u32) {
        self.output.push_back(PlayerMovementOutput::AreaTrigger {
            heartbeat,
            trigger_id,
        });
    }

    /// The scene camera feeds collision distance back into its retained zoom lane.
    pub(super) fn camera_obstructed(&mut self, distance: f32, time: u32) {
        if let Some(owner) = &mut self.owner {
            owner.camera.obstructed(distance, time);
        }
    }

    /// Initial zero-launch support resolution belongs behind the loading card.
    /// Server-authored airborne entry already has a valid presentation state.
    pub(super) fn initial_contact_ready(&self) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| !owner.initial_contact_pending)
    }

    pub(super) fn refresh_camera_settings(
        &mut self,
        revision: u64,
        number: impl Fn(&str) -> Option<f32>,
    ) {
        if self.camera_cvar_revision == Some(revision) {
            return;
        }
        self.camera_cvar_revision = Some(revision);
        self.camera_follow_settings = PlayerCameraFollowSettings::read(&number);
        self.set_camera_zoom_settings(PlayerCameraZoomSettings {
            speed: number("cameradistancemovespeed").unwrap_or(8.33),
            maximum: number("cameradistancemax").unwrap_or(15.),
            maximum_factor: number("cameradistancemaxfactor").unwrap_or(1.),
        });
    }

    pub(super) fn set_camera_zoom_settings(&mut self, settings: PlayerCameraZoomSettings) {
        self.camera_zoom_settings = settings;
    }
    pub(super) fn mouse_free_look(&self) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.camera.free_look())
    }

    pub(super) fn animation_mouse_turning(&self, world: Option<&ActiveWorld>) -> bool {
        self.input.mouse_turning()
            && self
                .owner
                .as_ref()
                .zip(world)
                .is_some_and(|(owner, world)| {
                    owner.camera.free_look() && owner.admission(world).turning
                })
    }

    pub(super) fn push_mouse_motion(
        &mut self,
        delta: [f32; 2],
        settings: PlayerCameraMouseSettings,
        timestamp_ms: u32,
    ) {
        self.commands.push_back(MovementCommand::MouseMotion {
            delta,
            settings,
            timestamp_ms,
        });
    }

    pub(super) fn push(&mut self, command: UiMovementCommand) {
        self.commands.push_back(MovementCommand::Input(command));
    }

    pub(super) fn push_control(&mut self, event: PlayerControlEvent) {
        self.commands.push_back(MovementCommand::Control(event));
    }

    pub(super) fn take_animation_event(&mut self) -> Option<UnitMovementAnimationEvent> {
        self.owner.as_mut()?.animation_events.pop_front()
    }

    /// The composition root calls this before presentation samples ECS.
    pub(super) fn service(
        &mut self,
        gameplay: &mut RuntimeGameplayCoordinator,
        terrain: &mut RuntimeTerrainCoordinator,
        objects: &RuntimeGameObjectPresentation,
        dimensions: Option<[f32; 3]>,
        now_ms: u32,
    ) -> Result<(), RuntimePlayerMovementError> {
        let Some(world) = gameplay.world() else {
            self.reset();
            return Ok(());
        };
        let guid = world.local_player_guid()?;
        let Some(identity) = world.object_identity(guid) else {
            self.reset();
            return Ok(());
        };
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.identity != identity)
        {
            self.reset();
        }
        let Some(movement) = world.movement_state(guid) else {
            return Ok(());
        };
        let Some(dimensions) = dimensions else {
            return Ok(());
        };
        let transform = world.local_player_transform()?;
        let mut geometry = RuntimeMovementGeometry::new(
            terrain,
            world,
            objects,
            // 71C930 sets client control on entry; 75E3D0 includes the high bit.
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut self.geometry,
        );
        if self.owner.is_none() {
            let Some(mut owner) =
                LocalMovement::new_in_geometry(identity, transform, movement, now_ms, &geometry)?
            else {
                return Ok(());
            };
            owner.camera =
                PlayerCameraInput::new(world.local_player_view()?, transform.orientation());
            self.output
                .push_back(PlayerMovementOutput::ActiveMover(guid));
            // Initial selection uses the same acquire response as later
            // control recovery. Its zero-launch fall resolves resident support;
            // waiting for a separate pre-grounding callback deadlocks entry.
            owner.acquire_mover(&mut self.output)?;
            owner.initial_contact_pending = movement.context().falling.is_none()
                && matches!(owner.phase, MovementPhase::Fall(_));
            self.owner = Some(owner);
        } else if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.published != (transform, movement))
        {
            // A received transform/movement block supersedes the last local
            // publication. Never integrate from a stale private position.
            let retained_control = self.owner.as_ref().map(|owner| {
                (
                    owner.active,
                    owner.client_control,
                    owner.stand_state,
                    owner.camera,
                    owner.passenger_clock,
                )
            });
            let Some(owner) =
                LocalMovement::new_in_geometry(identity, transform, movement, now_ms, &geometry)?
            else {
                return Ok(());
            };
            self.owner = Some(owner);
            if let (
                Some(owner),
                Some((active, client_control, stand_state, camera, passenger_clock)),
            ) = (self.owner.as_mut(), retained_control)
            {
                // Control and stance updates have their own ordered commands.
                // Reading the final packet-pump state here would apply them
                // before older input waiting in the same queue.
                owner.active = active;
                owner.client_control = client_control;
                owner.stand_state = stand_state;
                owner.camera = camera;
                owner.passenger_clock = passenger_clock;
            }
        }
        let Some(owner) = self.owner.as_mut() else {
            return Ok(());
        };
        owner.refresh_passenger(&mut geometry)?;
        let delta = now_ms.wrapping_sub(owner.time_ms);
        if delta > 250 {
            let skipped = delta - 250;
            owner.skip(skipped, &mut self.output);
            owner.time_ms = owner.time_ms.wrapping_add(skipped);
        }
        loop {
            let next = self.commands.front().copied();
            let command_time = next.map(|command| {
                if (command.timestamp_ms().wrapping_sub(owner.time_ms) as i32) < 0 {
                    owner.time_ms
                } else {
                    command.timestamp_ms()
                }
            });
            let end = command_time
                .filter(|time| {
                    (*time).wrapping_sub(owner.time_ms) <= now_ms.wrapping_sub(owner.time_ms)
                })
                .unwrap_or(now_ms);
            owner.advance_to(end, dimensions, &mut geometry, &mut self.output)?;
            if command_time != Some(end) {
                break;
            }
            let Some(command) = self.commands.pop_front() else {
                break;
            };
            if let MovementCommand::Input(UiMovementCommand {
                action: solarity_ui::UiMovementAction::CameraZoom { inward, amount },
                timestamp_ms,
            }) = command
            {
                owner
                    .camera
                    .zoom(inward, amount, timestamp_ms, self.camera_zoom_settings);
            } else {
                let previous = self.input.held_bits();
                owner.command(command, &mut self.input, world, &mut self.output)?;
                owner.camera.follow_input(
                    previous,
                    self.input.held_bits(),
                    now_ms,
                    &self.camera_follow_settings,
                    owner.flags,
                );
            }
        }
        owner.camera.sample_zoom(now_ms, self.camera_zoom_settings);
        owner.camera.sample_follow(now_ms);
        let (transform, movement) = owner.snapshot();
        owner.published = (transform, movement);
        world.set_local_player_view(owner.camera.view(owner.world_orientation()))?;
        gameplay.apply_local_movement(owner.identity, transform, movement, owner.stand_state)?;
        self.flush_output(gameplay)
    }

    /// Drains frozen output without rebuilding snapshots under writer backpressure.
    pub(super) fn flush_output(
        &mut self,
        gameplay: &RuntimeGameplayCoordinator,
    ) -> Result<(), RuntimePlayerMovementError> {
        while let Some(output) = self.output.front().copied() {
            if !gameplay.send_player_movement(output)? {
                break;
            }
            self.output.pop_front();
        }
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.owner = None;
        self.input = PlayerInputState::default();
        self.commands.clear();
        self.output.clear();
        self.camera_cvar_revision = None;
    }
}

impl LocalMovement {
    fn command(
        &mut self,
        command: MovementCommand,
        input: &mut PlayerInputState,
        world: &ActiveWorld,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        let guid = self.identity.guid();
        let admission = self.admission(world);
        let mut effects = Vec::with_capacity(5);
        let was_paired = input.paired_mouse_buttons();
        match command {
            MovementCommand::SupportRecheck { .. } => {
                return if self.active {
                    self.acquire_mover(output)
                } else {
                    Ok(())
                };
            }
            MovementCommand::MouseMotion {
                delta, settings, ..
            } => {
                self.camera.motion(delta, settings);
                if self.camera.free_look() && input.mouse_turning() && admission.turning {
                    self.set_mouse_facing(input, output)?;
                }
            }
            MovementCommand::Input(command) if self.active => {
                input.apply(command.action, admission, &mut |effect| {
                    effects.push(effect)
                });
            }
            MovementCommand::Input(command) => input.record_without_mover(command.action),
            MovementCommand::Control(PlayerControlEvent::StandState { state, .. }) => {
                self.stand_state = state;
                self.notify_animation(UnitMovementAnimationEventKind::Changed);
                // Player_C::6E2B30 refreshes held input when standing up.
                if state == 0 && self.active {
                    input.resolve(self.admission(world), &mut |effect| effects.push(effect));
                }
            }
            MovementCommand::Control(PlayerControlEvent::PlayerControl { enabled, .. }) => {
                self.client_control = enabled;
            }
            MovementCommand::Control(PlayerControlEvent::ActiveMover {
                previous, next, ..
            }) => {
                if previous == guid {
                    self.release_mover()?;
                    self.emit(WorldMovementKind::NotActiveMover, output)?;
                }
                self.active = next == guid;
                if next != 0 {
                    output.push_back(PlayerMovementOutput::ActiveMover(next));
                }
                if self.active {
                    self.acquire_mover(output)?;
                    input.resolve(self.admission(world), &mut |effect| effects.push(effect));
                }
            }
        }
        let admission = self.admission(world);
        if self.camera.free_look() && !input.mouse_free_look() {
            self.camera.set_sticky_camera(matches!(
                command,
                MovementCommand::Input(UiMovementCommand {
                    action: solarity_ui::UiMovementAction::CameraOrbitStop {
                        sticky_camera: true
                    },
                    ..
                })
            ));
        }
        self.camera
            .set_free_look(input.mouse_free_look(), self.world_orientation());
        if !was_paired && input.paired_mouse_buttons() && admission.turning {
            self.set_mouse_facing(input, output)?;
        }
        for effect in effects {
            self.apply(effect, admission, world, output)?;
        }
        Ok(())
    }

    fn set_mouse_facing(
        &mut self,
        input: &mut PlayerInputState,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        // 6EE3A0 event 19 -> 989B70 -> MSG_MOVE_SET_FACING. The fall
        // snapshot retains its launch direction when the body turns in air.
        let facing = self.passenger.map_or(self.camera.yaw(), |parent| {
            parent.frame.local_orientation(self.camera.yaw())
        });
        if (self.orientation - facing).abs() >= 0.000_000_953_674_3 {
            self.orientation = facing;
        }
        self.flags &= !0x30;
        input.clear_active_turn();
        self.reanchor()?;
        self.emit(WorldMovementKind::SetFacing, output)
    }

    fn new(
        identity: WorldObjectIdentity,
        transform: WorldTransform,
        movement: WorldMovementState,
        time_ms: u32,
    ) -> Result<Self, RuntimePlayerMovementError> {
        let flags = movement.flags() as u32 & 0x77ff_fdff;
        if movement.transport_guid().is_some() || flags & 0x4ae0_0000 != 0 {
            return Err(RuntimePlayerMovementError::UnsupportedMode { flags });
        }
        let context = movement.context();
        let secondary = (movement.flags() >> 32) as u16;
        let ground = MovementGroundTrajectory::new(
            flags & !0x1000,
            secondary & 8 != 0,
            transform.orientation(),
            movement.speeds(),
        )?;
        let phase = if let Some(fall) = context.falling {
            let mode = fall_mode(flags);
            let curve = MovementFallTrajectory::new(mode, fall.vertical_speed)?;
            MovementPhase::Fall(MovementFallState::new(MovementFallSnapshot {
                position: transform.position(),
                fall_time_ms: context.fall_time_ms,
                launch_height: transform.position().z
                    + curve.distance_at_millis(context.fall_time_ms)?,
                initial_downward_speed: fall.vertical_speed,
                horizontal_direction: Vec2::new(fall.direction_cos, fall.direction_sin),
                horizontal_speed: fall.horizontal_speed,
                direction: Vec3::new(fall.direction_cos, fall.direction_sin, 0.),
                mode,
                phase: if flags & 0x2000 != 0 {
                    MovementFallPhase::FallingFar
                } else {
                    MovementFallPhase::Falling
                },
            })?)
        } else {
            MovementPhase::Ground {
                step_anchor: context.spline_elevation,
            }
        };
        Ok(Self {
            passenger: None,
            passenger_seat: -1,
            passenger_clock: PassengerClock::default(),
            remote: false,
            remote_profile: None,
            blend: None,
            animation_events: VecDeque::new(),
            camera: PlayerCameraInput::new(
                solarity_ecs::PlayerViewState::default(),
                transform.orientation(),
            ),
            initial_contact_pending: false,
            active: true,
            client_control: true,
            stand_state: 0,
            identity,
            position: transform.position(),
            orientation: transform.orientation(),
            flags,
            secondary,
            speeds: movement.speeds(),
            context,
            retained_launch_height: match phase {
                MovementPhase::Fall(fall) => fall.snapshot().launch_height,
                MovementPhase::Ground { .. } => 0.,
            },
            retained_downward_speed: context.falling.map_or(0., |fall| fall.vertical_speed),
            anchor: transform.position(),
            elapsed_ms: 0,
            time_ms,
            heartbeat_ms: time_ms.wrapping_add(500),
            ground,
            yaw: solarity_systems::MovementYawTrajectory::new(
                flags,
                secondary & 8 != 0,
                transform.orientation(),
                movement.speeds().turn_rate(),
            )?,
            phase,
            published: (transform, movement),
        })
    }

    fn admission(&self, world: &ActiveWorld) -> PlayerInputAdmission {
        let alive = world
            .local_player_vitals()
            .is_some_and(|vitals| vitals.health() > 0);
        let flags = world.unit_flags(self.identity.guid()).unwrap_or_default();
        PlayerInputAdmission {
            translation: self.active
                && alive
                && self.flags & 0x100a00 == 0
                && self.stand_state != 7,
            turning: self.active && alive && flags.primary() & 0x40000 == 0,
            forced_forward: flags.secondary() & 0x40 != 0,
            yaw_during_mouselook: false,
            movement_flags: self.flags,
            secondary_flags: self.secondary,
        }
    }

    fn reanchor(&mut self) -> Result<(), RuntimePlayerMovementError> {
        self.anchor = self.position;
        self.elapsed_ms = 0;
        self.ground = MovementGroundTrajectory::new(
            self.flags & !0x1000,
            self.secondary & 8 != 0,
            self.orientation,
            self.speeds,
        )?;
        self.yaw = solarity_systems::MovementYawTrajectory::new(
            self.flags,
            self.secondary & 8 != 0,
            self.orientation,
            self.speeds.turn_rate(),
        )?;
        Ok(())
    }

    fn release_mover(&mut self) -> Result<(), RuntimePlayerMovementError> {
        // 6EE920 -> 6ED7E0 -> 6E9980 stops the previous subject without
        // ordinary movement packets, preserving walking and effect state.
        if self.flags & 0x1000 != 0 {
            self.flags &= !0x3000;
        }
        if self.flags & 0x100000 != 0 {
            self.flags = self.flags & 0xff203f00 | 0x800;
        }
        self.flags &= 0xfb303fff;
        self.flags &= !0x0000_f0ff;
        self.secondary &= 0xe3ff;
        if let MovementPhase::Fall(fall) = self.phase {
            self.context.fall_time_ms = fall.snapshot().fall_time_ms;
        }
        self.phase = MovementPhase::Ground { step_anchor: None };
        self.active = false;
        self.reanchor()
    }

    fn acquire_mover(
        &mut self,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        // 6EE870 reads virtual +0xA8 before admitting the acquire response.
        if let Some(time) = self.passenger.and_then(|parent| parent.time_ms) {
            self.passenger_clock.publish(time);
        }
        // 6EE870 queues event 9. Its 6EF860 admission and 98B710 ->
        // 988370 response start a zero-launch fall, even on level ground.
        // The next collision interval determines support beneath the mover.
        let Some(flags) = self.acquired_fall_flags() else {
            return Ok(());
        };
        self.reanchor()?;
        self.phase = MovementPhase::Fall(MovementFallState::new(MovementFallSnapshot {
            position: self.position,
            fall_time_ms: 0,
            launch_height: self.position.z,
            initial_downward_speed: 0.,
            horizontal_direction: self.ground.direction(),
            horizontal_speed: self.ground.speed(),
            direction: self.ground.direction().extend(0.),
            mode: fall_mode(self.flags),
            phase: MovementFallPhase::Falling,
        })?);
        self.flags = flags;
        self.retained_launch_height = self.position.z;
        self.retained_downward_speed = 0.;
        self.reanchor()?;
        self.emit(WorldMovementKind::Heartbeat, output)
    }

    fn acquired_fall_flags(&self) -> Option<u32> {
        if self.flags & 0x0230_1e00 != 0 || self.secondary & 4 != 0 {
            return None;
        }
        let mut flags = self.flags & 0xf91f_ffff | 0x1000;
        if self.secondary & 0x20 == 0 {
            flags &= !0xc0;
        }
        Some(flags)
    }

    fn request_stand(
        &mut self,
        state: u8,
        world: &ActiveWorld,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) {
        let alive = world
            .local_player_vitals()
            .is_some_and(|vitals| vitals.health() > 0);
        let flags = world.unit_flags(self.identity.guid()).unwrap_or_default();
        // 6DCB40's unit-field and movement gates. Cast, mount-spell and
        // cinematic ownership still need their complete runtime projections.
        if !alive
            || !self.client_control
            || flags.primary() & 0x100000 != 0
            || flags.secondary() & 1 != 0
            || (matches!(state, 1 | 3) && self.flags & 0x30 != 0)
            || (state != 0 && self.flags & 0x02e0_100f != 0)
        {
            return;
        }
        self.stand_state = state;
        self.notify_animation(UnitMovementAnimationEventKind::Changed);
        output.push_back(PlayerMovementOutput::StandState(u32::from(state)));
    }

    fn skip(&mut self, milliseconds: u32, output: &mut VecDeque<PlayerMovementOutput>) {
        if milliseconds == 0 || !self.active || self.remote {
            return;
        }
        self.heartbeat_ms = self.heartbeat_ms.wrapping_add(milliseconds);
        output.push_back(PlayerMovementOutput::SkippedTime {
            guid: self.identity.guid(),
            milliseconds,
        });
    }

    fn emit(
        &mut self,
        kind: WorldMovementKind,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        if self.remote {
            return Ok(());
        }
        let (transform, movement) = self.snapshot();
        let mut context = movement.context();
        if kind == WorldMovementKind::ChangeTransport && context.transport.is_none() {
            context.transport = Some(self.passenger_snapshot());
        }
        let mut flags = movement.flags();
        if let Some(transport) = context.transport {
            flags |= 0x200;
            if transport.interpolated_time_ms.is_some() {
                flags |= 0x400_u64 << 32;
            }
        }
        let packet = WorldMovementMessage::new(
            kind,
            self.identity.guid(),
            flags,
            transform.position().to_array(),
            transform.orientation(),
            ObjectMovementContext {
                timestamp_ms: context.timestamp_ms,
                transport: context.transport.map(|transport| {
                    solarity_network::ObjectMovementTransport {
                        guid: transport.guid,
                        position: transport.position.to_array(),
                        orientation: transport.orientation,
                        time_ms: transport.time_ms,
                        seat: transport.seat,
                        interpolated_time_ms: transport.interpolated_time_ms,
                    }
                }),
                pitch_radians: context.pitch_radians,
                fall_time_ms: context.fall_time_ms,
                falling: context.falling.map(|fall| ObjectMovementFall {
                    vertical_speed: fall.vertical_speed,
                    direction_sin: fall.direction_sin,
                    direction_cos: fall.direction_cos,
                    horizontal_speed: fall.horizontal_speed,
                }),
                spline_elevation: context.spline_elevation,
            },
        )?;
        if context.transport.is_some() {
            self.passenger_clock.serialized();
        }
        // Unit_C::73ED10 only resolves animation for these notifications.
        // Heartbeats and facing/pitch packets must not interrupt a landing.
        use WorldMovementKind as Kind;
        let animation = match kind {
            Kind::Jump => Some(UnitMovementAnimationEventKind::Jump),
            Kind::StartForward
            | Kind::StartBackward
            | Kind::Stop
            | Kind::StartStrafeLeft
            | Kind::StartStrafeRight
            | Kind::StopStrafe
            | Kind::StartTurnLeft
            | Kind::StartTurnRight
            | Kind::StopTurn
            | Kind::StartSwim
            | Kind::StopSwim
            | Kind::StartAscend
            | Kind::StopAscend
            | Kind::StartDescend => Some(UnitMovementAnimationEventKind::Changed),
            Kind::SetRunMode | Kind::SetWalkMode if self.flags & 0xc0100f != 0 => {
                Some(UnitMovementAnimationEventKind::Changed)
            }
            _ => None,
        };
        if let Some(animation) = animation {
            self.notify_animation(animation);
        }
        output.push_back(PlayerMovementOutput::Movement(packet));
        self.heartbeat_ms = self.time_ms.wrapping_add(500);
        Ok(())
    }

    fn notify_animation(&mut self, kind: UnitMovementAnimationEventKind) {
        self.animation_events.push_back(UnitMovementAnimationEvent {
            identity: self.identity,
            movement: self.snapshot().1,
            stand: self.stand_state,
            kind,
        });
    }

    fn snapshot(&self) -> (WorldTransform, WorldMovementState) {
        let mut context = self.context;
        context.timestamp_ms = self.time_ms;
        context.transport = self.passenger.map(|_| self.passenger_snapshot());
        context.spline_elevation = match self.phase {
            MovementPhase::Ground { step_anchor } => step_anchor,
            MovementPhase::Fall(_) => None,
        };
        context.falling = match self.phase {
            MovementPhase::Fall(fall) => {
                let fall = fall.snapshot();
                context.fall_time_ms = fall.fall_time_ms;
                Some(WorldMovementFall {
                    vertical_speed: fall.initial_downward_speed,
                    direction_sin: fall.horizontal_direction.y,
                    direction_cos: fall.horizontal_direction.x,
                    horizontal_speed: fall.horizontal_speed,
                })
            }
            MovementPhase::Ground { .. } => None,
        };
        (
            WorldTransform::new(self.world_position(), self.world_orientation()),
            WorldMovementState::new(
                u64::from(self.flags & 0x77ff_fdff)
                    | (u64::from(self.secondary & !0x400) << 32)
                    | if context.transport.is_some() {
                        0x200
                    } else {
                        0
                    }
                    | if context
                        .transport
                        .is_some_and(|transport| transport.interpolated_time_ms.is_some())
                    {
                        0x400_u64 << 32
                    } else {
                        0
                    },
                self.speeds,
                context,
            ),
        )
    }

    fn advance_to<G: LocalMovementGeometry>(
        &mut self,
        end: u32,
        dimensions: [f32; 3],
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        while self.time_ms != end {
            let mut duration = end.wrapping_sub(self.time_ms);
            if self.flags & 0xc0100f != 0 {
                duration = duration.min(self.heartbeat_ms.wrapping_sub(self.time_ms));
            }
            if duration != 0 {
                self.interval(duration, dimensions, geometry, output)?;
                self.time_ms = self.time_ms.wrapping_add(duration);
            }
            if self.flags & 0xc0100f != 0
                && (self.time_ms.wrapping_sub(self.heartbeat_ms) as i32) >= 0
            {
                self.emit(WorldMovementKind::Heartbeat, output)?;
            }
        }
        Ok(())
    }

    fn interval<G: LocalMovementGeometry>(
        &mut self,
        duration: u32,
        dimensions: [f32; 3],
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        if !self.active {
            return Ok(());
        }
        if self.flags & 0x800 != 0 {
            self.heartbeat_ms = self.heartbeat_ms.wrapping_add(duration);
            return Ok(());
        }
        if self.flags & 0x10ff == 0 {
            return Ok(());
        }
        let [radius, height, step_height] = dimensions;
        let profile = self
            .remote_profile
            .unwrap_or(MovementGroundProfile::PlayerControlled { step_height });
        let mut remaining = duration;
        while remaining != 0 {
            self.elapsed_ms = self.elapsed_ms.wrapping_add(remaining);
            let sample = self.ground.sample(self.elapsed_ms);
            self.orientation = self.yaw.sample(self.elapsed_ms);
            let mut delta = match self.phase {
                MovementPhase::Ground { .. } => self.anchor + sample.displacement - self.position,
                MovementPhase::Fall(fall) => {
                    let state = fall.snapshot();
                    let seconds = (f64::from(self.elapsed_ms)
                        * f64::from(f32::from_bits(0x3a83_126f)))
                        as f32;
                    (self.anchor
                        + (state.direction.as_dvec3()
                            * f64::from(seconds)
                            * f64::from(state.horizontal_speed))
                        .as_vec3())
                        - self.position
                }
            };
            let mut blended_position = false;
            if let Some(blend) = &mut self.blend {
                if let MovementPhase::Fall(fall) = self.phase {
                    let fall = fall.snapshot();
                    delta.z = MovementFallTrajectory::new(fall.mode, fall.initial_downward_speed)?
                        .vertical_displacement(
                            fall.fall_time_ms.wrapping_add(remaining),
                            self.position.z,
                            fall.launch_height,
                        )?;
                }
                let mut analytic = solarity_systems::RemoteMovementPose {
                    transform: WorldTransform::new(self.position + delta, self.orientation),
                    pitch: self.context.pitch_radians.unwrap_or(0.0),
                    transport_guid: 0,
                };
                blended_position = blend.sample(
                    self.position,
                    self.time_ms.wrapping_add(duration - remaining),
                    remaining,
                    &mut analytic,
                );
                delta = analytic.transform.position() - self.position;
                self.orientation = analytic.transform.orientation();
                if self.context.pitch_radians.is_some() {
                    self.context.pitch_radians = Some(analytic.pitch);
                }
            }
            if self.flags & 0x100f == 0 {
                return Ok(());
            }
            let distance = delta.truncate().as_dvec2().length() as f32;
            let direction = if distance.abs() >= f32::from_bits(0x3580_0000) {
                (delta.truncate().as_dvec2() * (1.0 / f64::from(distance))).as_vec2()
            } else {
                Vec2::ZERO
            };
            let mode = match self.phase {
                MovementPhase::Ground { .. } => MovementIntervalMode::Grounded(profile),
                MovementPhase::Fall(fall) => {
                    let fall = fall.snapshot();
                    MovementIntervalMode::Airborne {
                        fall_time_ms: fall.fall_time_ms,
                        launch_height: fall.launch_height,
                        trajectory: MovementFallTrajectory::new(
                            fall.mode,
                            fall.initial_downward_speed,
                        )?,
                    }
                }
            };
            if !geometry.collect(MovementIntervalRequest {
                position: self.position,
                radius,
                height,
                distance,
                direction: direction.extend(0.),
                duration_ms: remaining,
                mode,
            })? {
                self.elapsed_ms = self.elapsed_ms.wrapping_sub(remaining);
                self.skip(remaining, output);
                return Ok(());
            }
            // 7618B0 checks an existing parent's retention before airborne motion.
            // A leave consumes no collision time and terminates this interval.
            if matches!(self.phase, MovementPhase::Fall(_))
                && self.passenger.is_some()
                && self.contact_passenger(0, geometry)?
            {
                self.elapsed_ms = self.elapsed_ms.saturating_sub(remaining);
                let saved = self.time_ms;
                self.time_ms = saved.wrapping_add(duration - remaining);
                self.emit(WorldMovementKind::ChangeTransport, output)?;
                self.time_ms = saved;
                return Ok(());
            }
            let was_airborne = matches!(self.phase, MovementPhase::Fall(_));
            let mut landing = None;
            let (consumed, reset, skipped, contact) = match self.phase {
                MovementPhase::Ground { step_anchor } => {
                    let current_basis = MovementGroundTrajectory::new(
                        self.flags,
                        self.secondary & 8 != 0,
                        self.orientation,
                        self.speeds,
                    )?;
                    let state = MovementGroundState::new(MovementGroundSnapshot {
                        position: self.position,
                        step_anchor,
                        fall_time_ms: self.context.fall_time_ms,
                        launch_height: self.retained_launch_height,
                        initial_downward_speed: self.retained_downward_speed,
                        horizontal_direction: current_basis.direction(),
                        horizontal_speed: self.ground.speed(),
                        direction: current_basis.direction().extend(0.),
                        mode: fall_mode(self.flags),
                        fall_admission: if self.flags & 0x2201e00 == 0 && self.secondary & 4 == 0 {
                            MovementFallAdmission::Allowed
                        } else {
                            MovementFallAdmission::Suppressed
                        },
                    })?;
                    let result = state.advance_with_geometry(
                        MovementGroundInterval {
                            duration_ms: remaining,
                            distance,
                            direction,
                            radius,
                            height,
                            profile,
                        },
                        geometry,
                    )?;
                    match result.continuation {
                        MovementGroundContinuation::Grounded(state) => {
                            let state = state.snapshot();
                            self.position = state.position;
                            self.context.fall_time_ms = state.fall_time_ms;
                            self.retained_launch_height = state.launch_height;
                            self.retained_downward_speed = state.initial_downward_speed;
                            self.phase = MovementPhase::Ground {
                                step_anchor: state.step_anchor,
                            };
                            if state.step_anchor.is_some() {
                                self.flags |= 0x0400_0000;
                            } else {
                                self.flags &= !0x0400_0000;
                            }
                        }
                        MovementGroundContinuation::Falling(fall) => {
                            self.position = fall.snapshot().position;
                            self.retained_launch_height = fall.snapshot().launch_height;
                            self.retained_downward_speed = fall.snapshot().initial_downward_speed;
                            self.phase = MovementPhase::Fall(fall);
                            self.flags = self.flags & 0xf91f_ff3f | 0x1000;
                        }
                    }
                    (
                        result.consumed_ms,
                        result.reset_motion_anchor,
                        result.skipped_time_ms,
                        result.contact_triangle,
                    )
                }
                MovementPhase::Fall(fall) => {
                    let state = fall.snapshot();
                    let curve =
                        MovementFallTrajectory::new(state.mode, state.initial_downward_speed)?;
                    let vertical = curve.vertical_displacement(
                        state.fall_time_ms.wrapping_add(remaining),
                        self.position.z,
                        state.launch_height,
                    )?;
                    let displacement = (direction * distance).extend(if blended_position {
                        delta.z
                    } else {
                        vertical
                    });
                    let result = fall.advance_with_geometry(
                        MovementFallInterval {
                            duration_ms: remaining,
                            displacement,
                            radius,
                            height,
                            support_profile: match profile {
                                MovementGroundProfile::PlayerControlled { .. } => {
                                    MovementSupportProfile::PlayerControlled
                                }
                                MovementGroundProfile::Other => MovementSupportProfile::Other,
                            },
                            policy: if self.flags & 0xf == 0 {
                                MovementFallAdvancePolicy::Live
                            } else {
                                MovementFallAdvancePolicy::LiveTranslating
                            },
                        },
                        geometry,
                    )?;
                    match result.continuation {
                        MovementFallContinuation::Airborne(fall) => {
                            self.position = fall.snapshot().position;
                            self.retained_launch_height = fall.snapshot().launch_height;
                            self.retained_downward_speed = fall.snapshot().initial_downward_speed;
                            self.phase = MovementPhase::Fall(fall);
                            if fall.snapshot().phase == MovementFallPhase::FallingFar {
                                self.flags |= 0x2000;
                            }
                        }
                        MovementFallContinuation::Landed {
                            position,
                            fall_time_ms,
                        } => {
                            let previous_flags = self.flags;
                            self.initial_contact_pending = false;
                            self.position = position;
                            self.context.fall_time_ms = fall_time_ms;
                            self.phase = MovementPhase::Ground { step_anchor: None };
                            self.flags &= !0x3000;
                            self.apply_deferred();
                            self.reanchor()?;
                            landing = Some(UnitMovementAnimationEventKind::Land {
                                previous_flags,
                                forced: state.initial_downward_speed != 0.0,
                                slow: self.ground.speed() <= self.speeds.walk() * 2.0,
                            });
                        }
                    }
                    (
                        result.consumed_ms,
                        result.reset_motion_anchor,
                        result.skipped_time_ms,
                        result.contact_triangle,
                    )
                }
            };
            geometry.check_failure()?;
            if !was_airborne && (reset || blended_position) {
                self.reanchor()?;
            }
            let contact_guid = geometry.contact_guid(contact);
            // 7620F0 reports even a static zero GUID; 7612B0 only reports a
            // nonzero parent after its airborne contact response.
            let changed = contact.is_some()
                && (!was_airborne || contact_guid != 0)
                && self.contact_passenger(contact_guid, geometry)?;
            if was_airborne && (reset || blended_position) {
                self.reanchor()?;
            }
            if !(reset || blended_position) {
                self.elapsed_ms = self.elapsed_ms.wrapping_sub(skipped);
            }
            let saved = self.time_ms;
            self.time_ms = saved.wrapping_add(duration - remaining + consumed);
            // 6EB0B0 gives the landing packet precedence over ChangeTransport.
            if let Some(landing) = landing {
                self.notify_animation(landing);
                self.emit(WorldMovementKind::FallLand, output)?;
            } else if changed {
                self.emit(WorldMovementKind::ChangeTransport, output)?;
            }
            self.time_ms = saved;
            self.skip(skipped, output);
            remaining = remaining.saturating_sub(consumed);
            if changed || landing.is_some() {
                self.elapsed_ms = self.elapsed_ms.saturating_sub(remaining);
                return Ok(());
            }
        }
        Ok(())
    }

    fn apply_deferred(&mut self) {
        let flags = self.flags;
        if flags & 0x4000 != 0 {
            self.flags &= !3;
        }
        if flags & 0x10000 != 0 {
            self.flags = self.flags & !2 | 1;
        } else if flags & 0x20000 != 0 {
            self.flags = self.flags & !1 | 2;
        }
        if flags & 0x8000 != 0 {
            self.flags &= !0xc;
        }
        if flags & 0x40000 != 0 && self.secondary & 1 == 0 {
            self.flags = self.flags & !8 | 4;
        } else if flags & 0x80000 != 0 && self.secondary & 1 == 0 {
            self.flags = self.flags & !4 | 8;
        }
        self.flags &= 0xffe0_3fff;
    }

    fn apply(
        &mut self,
        effect: PlayerInputEffect,
        admission: PlayerInputAdmission,
        world: &ActiveWorld,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        use WorldMovementKind as Kind;
        let kind = match effect {
            PlayerInputEffect::Movement(kind) => kind,
            PlayerInputEffect::ToggleRun => {
                if !admission.translation {
                    return Ok(());
                }
                if self.flags & 0x100 != 0 {
                    Kind::SetRunMode
                } else {
                    Kind::SetWalkMode
                }
            }
            PlayerInputEffect::SitStand => {
                self.request_stand(u8::from(self.stand_state == 0), world, output);
                return Ok(());
            }
            PlayerInputEffect::Jump => {
                if !admission.translation
                    || self.flags & 0x0200_1c00 != 0
                    || self.secondary & 2 != 0
                {
                    return Ok(());
                }
                self.reanchor()?;
                self.phase = MovementPhase::Fall(MovementFallState::new(MovementFallSnapshot {
                    position: self.position,
                    fall_time_ms: 0,
                    launch_height: self.position.z,
                    initial_downward_speed: f32::from_bits(0xc0fe_93d8),
                    horizontal_direction: self.ground.direction(),
                    horizontal_speed: self.ground.speed(),
                    direction: self.ground.direction().extend(0.),
                    mode: fall_mode(self.flags),
                    phase: MovementFallPhase::Falling,
                })?);
                self.flags = self.flags & 0xf91f_ff3f | 0x1000;
                self.retained_launch_height = self.position.z;
                self.retained_downward_speed = f32::from_bits(0xc0fe_93d8);
                self.reanchor()?;
                if self.stand_state != 0 {
                    self.request_stand(0, world, output);
                }
                return self.emit(Kind::Jump, output);
            }
        };
        let airborne = matches!(self.phase, MovementPhase::Fall(_));
        let mut reanchor = true;
        match kind {
            Kind::StartForward | Kind::StartBackward => {
                self.flags &= !0x0003_4000;
                let (bit, pending) = if kind == Kind::StartForward {
                    (1, 0x10000)
                } else {
                    (2, 0x20000)
                };
                if airborne && self.flags & 0xf != 0 {
                    if self.flags & bit == 0 {
                        self.flags |= pending;
                    }
                    reanchor = false;
                } else {
                    self.flags = self.flags & !3 | bit;
                }
            }
            Kind::Stop => {
                self.flags &= !0x30000;
                if airborne {
                    self.flags |= 0x4000;
                    reanchor = false;
                } else {
                    self.flags &= !3;
                }
            }
            Kind::StartStrafeLeft | Kind::StartStrafeRight => {
                self.flags &= !0x000c_8000;
                if self.secondary & 1 == 0 {
                    let (bit, pending) = if kind == Kind::StartStrafeLeft {
                        (4, 0x40000)
                    } else {
                        (8, 0x80000)
                    };
                    if airborne && self.flags & 0xf != 0 {
                        if self.flags & bit == 0 {
                            self.flags |= pending;
                        }
                        reanchor = false;
                    } else {
                        self.flags = self.flags & !0xc | bit;
                    }
                } else {
                    reanchor = false;
                }
            }
            Kind::StopStrafe => {
                self.flags &= !0xc0000;
                if airborne {
                    self.flags |= 0x8000;
                    reanchor = false;
                } else {
                    self.flags &= !0xc;
                }
            }
            Kind::StartTurnLeft => self.flags = self.flags & !0x20 | 0x10,
            Kind::StartTurnRight => self.flags = self.flags & !0x10 | 0x20,
            Kind::StopTurn => self.flags &= !0x30,
            Kind::SetRunMode => self.flags &= !0x100,
            Kind::SetWalkMode => self.flags |= 0x100,
            _ => return Err(RuntimePlayerMovementError::UnsupportedMode { flags: self.flags }),
        }
        if reanchor {
            self.reanchor()?;
            if airborne && self.flags & 0xf != 0 {
                // Native can initiate a walking-speed horizontal launch from a
                // stationary jump; established launch motion remains retained.
                if let MovementPhase::Fall(fall) = self.phase
                    && fall.snapshot().horizontal_speed == 0.0
                {
                    let mut fall = fall.snapshot();
                    fall.horizontal_direction = self.ground.direction();
                    fall.direction = fall.horizontal_direction.extend(0.);
                    fall.horizontal_speed = self.speeds.walk().min(self.speeds.run());
                    self.phase = MovementPhase::Fall(MovementFallState::new(fall)?);
                }
            }
        }
        if self.stand_state != 0
            && matches!(
                kind,
                Kind::StartForward
                    | Kind::StartBackward
                    | Kind::StartStrafeLeft
                    | Kind::StartStrafeRight
            )
        {
            self.request_stand(0, world, output);
        }
        self.emit(kind, output)
    }
}

fn fall_mode(flags: u32) -> MovementFallMode {
    if flags & 0x2000_0000 != 0 {
        MovementFallMode::Slow
    } else {
        MovementFallMode::Normal
    }
}

#[cfg(test)]
#[path = "player_movement/tests.rs"]
mod tests;
