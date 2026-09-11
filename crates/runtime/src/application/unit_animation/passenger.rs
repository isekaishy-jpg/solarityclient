//! Vehicle passenger model state survives replacement of the unit's model.

use glam::{Mat4, Vec3};
use solarity_asset::{DecodedM2Model, VehicleCatalog, VehicleSeatDefinition};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldObjectIdentity, WorldTransform};
use solarity_rendering::M2AnimationClock;
use solarity_systems::{
    VehiclePassengerAnimationInput, VehiclePassengerPhase as Phase, VehiclePassengerTransition,
    VehicleTransitionInput,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use super::{
    UnitAnimationBehavior, UnitAnimationScene, UnitMovementAnimationEvent,
    UnitMovementAnimationEventKind,
};
use crate::application::unit_passenger::UnitPassengerFrames;

#[cfg(test)]
#[path = "../../../tests/application/vehicle_transition.rs"]
mod tests;

pub(super) type SharedPassengerState = Rc<RefCell<UnitPassengerModel>>;

#[derive(Clone, Copy)]
pub(in crate::application) struct UnitPassengerModelInput {
    pub parent: WorldObjectIdentity,
    pub parent_live: bool,
    pub seat: Option<VehicleSeatDefinition>,
    pub parent_pose: WorldTransform,
    pub parent_frame: Mat4,
    pub parent_velocity: Vec3,
}

/// A CPU passenger can request a vehicle bone before its own model exists.
#[derive(Clone)]
pub(in crate::application) struct UnitPassengerController {
    identity: WorldObjectIdentity,
    state: SharedPassengerState,
}

#[derive(Clone, Copy)]
pub(in crate::application) struct UnitPassengerTarget {
    pub parent: UnitPassengerModelInput,
    pub yaw: f32,
    pub anchor: Option<Vec3>,
    pub scale: f32,
}

impl UnitPassengerController {
    pub fn identity(&self) -> WorldObjectIdentity {
        self.identity
    }

    pub fn needs_advance(&self, now_ms: u32) -> bool {
        let state = self.state.borrow();
        !matches!(state.phase, Phase::Detached | Phase::Seated)
            && state.timing.is_none_or(|timing| timing.finished(now_ms))
    }

    pub fn fallback_position(&self) -> Vec3 {
        let state = self.state.borrow();
        state.unit_pose.map_or(state.origin, |pose| pose.position())
    }

    pub fn target(&self) -> Option<UnitPassengerTarget> {
        let state = self.state.borrow();
        if !matches!(state.phase, Phase::EnterDelay | Phase::Entering) {
            return None;
        }
        let parent = state.input?;
        Some(UnitPassengerTarget {
            parent,
            yaw: state.local_yaw.unwrap_or(0.),
            // An absent model does not mark the anchor lookup as complete.
            // 748400 retries it after the child's actual model has loaded.
            anchor: state.anchor.flatten(),
            scale: state
                .last_transform
                .map_or(1., |pose| pose.x_axis.truncate().length()),
        })
    }

    pub fn advance(&self, now_ms: u32, target: Vec3) {
        let mut state = self.state.borrow_mut();
        let velocity = state
            .input
            .map_or(Vec3::ZERO, |input| input.parent_velocity);
        state.advance(now_ms, target, velocity);
    }
}

#[derive(Default)]
pub(super) struct UnitPassengerModel {
    requested: Option<(u64, i8)>,
    input: Option<UnitPassengerModelInput>,
    last_transform: Option<Mat4>,
    last_yaw: Option<f32>,
    anchor: Option<Option<Vec3>>,
    old_input: Option<UnitPassengerModelInput>,
    pending: VecDeque<UnitMovementAnimationEvent>,
    phase: Phase,
    timing: Option<VehiclePassengerTransition>,
    origin: Vec3,
    origin_yaw: f32,
    local_yaw: Option<f32>,
    retained_seat_yaw: f32,
    origin_parent: Option<(WorldObjectIdentity, Vec3)>,
    phase_start_ms: u32,
    unit_pose: Option<WorldTransform>,
    animation_changed: bool,
    special_exit: bool,
    animation_completed: u32,
    animation_reset_pending: bool,
}

impl UnitAnimationScene {
    pub fn collect_passenger_transitions(&self, output: &mut Vec<UnitPassengerController>) {
        output.clear();
        output.extend(
            self.passenger_states
                .borrow()
                .values()
                .filter(|(_, state)| {
                    !matches!(state.borrow().phase, Phase::Detached | Phase::Seated)
                })
                .map(|(identity, state)| UnitPassengerController {
                    identity: *identity,
                    state: Rc::clone(state),
                }),
        );
    }

    /// Receipt admission runs before local held controls can resume. It must not
    /// read the still-unpublished ECS movement image or start a second timer.
    pub fn admit_movement_passenger(
        &self,
        world: &ActiveWorld,
        frames: &UnitPassengerFrames,
        event: UnitMovementAnimationEvent,
    ) {
        self.passenger_state(event.identity)
            .borrow_mut()
            .admit_event(world, frames.vehicles(), frames, event, self.scene_time_ms);
    }

    /// 74BA40 rejects client-controlled input in all four delay/travel phases.
    pub fn passenger_input_blocked(&self, identity: WorldObjectIdentity) -> bool {
        self.passenger_states
            .borrow()
            .get(&identity.guid())
            .is_some_and(|(generation, state)| {
                *generation == identity
                    && !matches!(state.borrow().phase, Phase::Detached | Phase::Seated)
            })
    }

    pub(super) fn passenger_state(&self, identity: WorldObjectIdentity) -> SharedPassengerState {
        let mut states = self.passenger_states.borrow_mut();
        let entry = states.entry(identity.guid()).or_insert_with(|| {
            (
                identity,
                Rc::new(RefCell::new(UnitPassengerModel::default())),
            )
        });
        if entry.0 != identity {
            *entry = (
                identity,
                Rc::new(RefCell::new(UnitPassengerModel::default())),
            );
        }
        Rc::clone(&entry.1)
    }

    /// CPU passenger ownership precedes and survives the unit's model binding.
    #[cfg(test)]
    pub fn synchronize_passengers(
        &self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
    ) {
        self.synchronize_passenger_inputs(world, vehicles, frames);
        self.advance_unbound_passengers_without_scene();
    }

    /// Admission is separate from timing so a newly resident vehicle can supply
    /// its attachment before an unloaded passenger initializes its travel.
    pub fn synchronize_passenger_inputs(
        &self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
    ) {
        for (identity, state) in self.passenger_states.borrow().values() {
            if world.object_identity(identity.guid()) == Some(*identity) {
                state.borrow_mut().synchronize(
                    world,
                    vehicles,
                    frames,
                    *identity,
                    None,
                    self.scene_time_ms,
                );
            }
        }
    }

    /// With no model scene, the existing no-parent-model target remains valid.
    pub fn advance_unbound_passengers_without_scene(&self) {
        for (_, state) in self.passenger_states.borrow().values() {
            let mut state = state.borrow_mut();
            let target = state.unit_pose.map_or(state.origin, |pose| pose.position());
            let velocity = state
                .input
                .map_or(Vec3::ZERO, |input| input.parent_velocity);
            state.advance(self.scene_time_ms, target, velocity);
        }
    }
}

impl UnitPassengerModel {
    pub(super) fn queue(&mut self, event: UnitMovementAnimationEvent) {
        self.pending.push_back(event);
    }

    fn admit_event(
        &mut self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
        event: UnitMovementAnimationEvent,
        now_ms: u32,
    ) {
        let UnitMovementAnimationEventKind::Passenger {
            previous_transform,
            previous,
            parent,
            animated,
        } = event.kind
        else {
            return;
        };
        let requested = event
            .movement
            .context()
            .transport
            .filter(|transport| transport.guid != 0)
            .map(|transport| (transport.guid, transport.seat));
        // Creation before model loading can still supply the old seat row.
        if self.input.is_none() {
            self.input = resolve_input(world, vehicles, frames, previous, None);
            if self.input.is_some() {
                self.phase = Phase::Seated;
            }
        }
        self.special_exit = event.movement.flags() >> 32 & 0x40 != 0;
        self.change(
            world,
            vehicles,
            frames,
            requested,
            parent,
            animated,
            previous_transform,
            now_ms,
        );
    }

    fn synchronize(
        &mut self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
        identity: WorldObjectIdentity,
        admitted_parent: Option<WorldObjectIdentity>,
        now_ms: u32,
    ) {
        self.unit_pose = world.object_transform(identity.guid());
        while let Some(event) = self.pending.pop_front() {
            self.admit_event(world, vehicles, frames, event, now_ms);
        }
        let requested = world
            .movement_state(identity.guid())
            .and_then(|movement| movement.context().transport)
            .filter(|transport| transport.guid != 0)
            .map(|transport| (transport.guid, transport.seat));
        if self.requested != requested {
            self.special_exit = world
                .movement_state(identity.guid())
                .is_some_and(|movement| movement.flags() >> 32 & 0x40 != 0);
            self.change(
                world,
                vehicles,
                frames,
                requested,
                admitted_parent,
                false,
                self.unit_pose
                    .unwrap_or(WorldTransform::new(Vec3::ZERO, 0.)),
                now_ms,
            );
        }
        self.local_yaw = world
            .movement_state(identity.guid())
            .and_then(|movement| movement.context().transport)
            .map(|transport| transport.orientation)
            .or_else(|| self.unit_pose.map(|pose| pose.orientation()));
        let Some((guid, slot)) = requested else {
            refresh_input(world, frames, &mut self.input);
            refresh_input(world, frames, &mut self.old_input);
            return;
        };
        // An admitted generation remains bound until an explicit link change.
        // A newly arriving parent can satisfy a previously unresolved request.
        let parent = admitted_parent
            .filter(|parent| parent.guid() == guid)
            .or_else(|| self.input.map(|input| input.parent))
            .or_else(|| world.object_identity(guid));
        let Some(parent) = parent else {
            return;
        };
        let live = world.object_identity(guid) == Some(parent);
        if !live {
            if let Some(input) = self.input.as_mut() {
                input.parent_live = false;
            }
            return;
        }
        if !matches!(
            world.object_kind(guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        ) {
            self.input = None;
            return;
        }
        let seat = world
            .unit_vehicle(guid)
            .and_then(|vehicle| vehicles.passenger_seat(vehicle.definition_id(), slot));
        if !self
            .input
            .is_some_and(|input| input.seat == seat && input.parent == parent)
        {
            self.anchor = None;
        }
        self.input = Some(UnitPassengerModelInput {
            parent,
            parent_live: true,
            seat,
            parent_pose: world
                .object_transform(guid)
                .unwrap_or(WorldTransform::new(Vec3::ZERO, 0.)),
            parent_frame: frames
                .current_frame(parent)
                .map_or(Mat4::IDENTITY, |frame| frame.world_matrix()),
            parent_velocity: frames.velocity(parent),
        });
        if self.phase == Phase::Detached {
            self.enter_phase(
                Phase::Seated,
                self.unit_pose
                    .unwrap_or(WorldTransform::new(Vec3::ZERO, 0.)),
                now_ms,
            );
        }
        refresh_input(world, frames, &mut self.old_input);
    }

    #[allow(clippy::too_many_arguments)]
    fn change(
        &mut self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
        requested: Option<(u64, i8)>,
        parent: Option<WorldObjectIdentity>,
        animated: bool,
        previous_pose: WorldTransform,
        now_ms: u32,
    ) {
        match self.phase {
            Phase::Detached | Phase::Exiting => self.old_input = None,
            Phase::Entering | Phase::Seated => self.old_input = self.input,
            Phase::EnterDelay | Phase::ExitDelay => {}
        }
        let next = resolve_input(world, vehicles, frames, requested, parent);
        let entering = next.is_some();
        // 74B200 retains the former seat row when the new GUID is zero.
        self.input = if requested.is_none() {
            self.input
        } else {
            next
        };
        self.requested = requested;
        self.anchor = None;
        let phase = self.input.and_then(|input| input.seat).map_or(
            if entering {
                Phase::Seated
            } else {
                Phase::Detached
            },
            |seat| {
                Phase::admit(
                    entering,
                    animated,
                    seat.flags(),
                    self.special_exit,
                    if entering {
                        seat.enter_transition()
                    } else {
                        seat.exit_transition()
                    },
                )
            },
        );
        self.enter_phase(phase, previous_pose, now_ms);
    }

    fn enter_phase(&mut self, phase: Phase, fallback: WorldTransform, now_ms: u32) {
        let previous = self.phase;
        if !(previous == Phase::EnterDelay && phase == Phase::Entering
            || previous == Phase::ExitDelay)
        {
            self.animation_completed = 0;
            self.animation_reset_pending = true;
        }
        self.phase = phase;
        self.phase_start_ms = now_ms;
        self.timing = None;
        self.origin = self
            .last_transform
            .map_or(fallback.position(), |matrix| matrix.w_axis.truncate());
        let origin_input = if matches!(phase, Phase::EnterDelay | Phase::ExitDelay) {
            self.old_input
        } else {
            self.input.filter(|input| {
                self.requested
                    .is_some_and(|(guid, _)| guid == input.parent.guid())
            })
        };
        self.origin_parent = origin_input
            .filter(|input| input.parent_live)
            .map(|input| (input.parent, self.origin - input.parent_pose.position()));
        if matches!(
            phase,
            Phase::Entering | Phase::EnterDelay | Phase::Exiting | Phase::ExitDelay
        ) {
            self.retained_seat_yaw = if matches!(previous, Phase::Detached | Phase::Seated) {
                self.local_yaw.unwrap_or_else(|| {
                    fallback.orientation()
                        - self
                            .old_input
                            .map_or(0., |input| input.parent_pose.orientation())
                })
            } else {
                self.last_yaw.unwrap_or(self.origin_yaw)
            };
            self.origin_yaw = if matches!(previous, Phase::Detached | Phase::Seated) {
                self.last_yaw.unwrap_or(fallback.orientation())
            } else {
                self.last_yaw.unwrap_or(self.origin_yaw)
            };
        }
        if phase == Phase::Seated {
            self.old_input = self.input;
        }
        if phase == Phase::Detached {
            self.input = None;
            self.old_input = None;
        }
        self.animation_changed = true;
    }

    fn advance(&mut self, now_ms: u32, target: Vec3, parent_velocity: Vec3) {
        if matches!(self.phase, Phase::Detached | Phase::Seated) {
            return;
        }
        let Some(seat) = self.input.and_then(|input| input.seat) else {
            return;
        };
        let pose = self
            .unit_pose
            .unwrap_or(WorldTransform::new(self.origin, self.origin_yaw));
        if self.timing.is_none() {
            let entering = matches!(self.phase, Phase::EnterDelay | Phase::Entering);
            let target = if self.phase == Phase::Entering {
                target
            } else {
                pose.position()
            };
            if target.distance_squared(self.origin) > 22_500. {
                self.enter_phase(
                    if entering {
                        Phase::Seated
                    } else {
                        Phase::Detached
                    },
                    pose,
                    now_ms,
                );
                return;
            }
            self.timing = Some(VehiclePassengerTransition::new(VehicleTransitionInput {
                phase: self.phase,
                has_parent: self.input.is_some_and(|input| input.parent_live),
                parameters: if entering {
                    seat.enter_transition()
                } else {
                    seat.exit_transition()
                },
                origin: self.origin,
                target,
                unit_position: pose.position(),
                parent_velocity,
                yaw: pose.orientation(),
                previous_yaw: self.origin_yaw,
                start_ms: self.phase_start_ms,
            }));
        }
        if self.timing.is_some_and(|timing| timing.finished(now_ms)) {
            let next = match self.phase {
                Phase::EnterDelay => Phase::Entering,
                Phase::Entering => Phase::Seated,
                Phase::ExitDelay => Phase::Exiting,
                Phase::Exiting => Phase::Detached,
                phase => phase,
            };
            self.enter_phase(next, pose, now_ms);
            if next.airborne() {
                self.advance(now_ms, target, parent_velocity);
            }
        }
    }
}

fn resolve_input(
    world: &ActiveWorld,
    vehicles: &VehicleCatalog,
    frames: &UnitPassengerFrames,
    requested: Option<(u64, i8)>,
    admitted: Option<WorldObjectIdentity>,
) -> Option<UnitPassengerModelInput> {
    let (guid, slot) = requested?;
    let parent = admitted
        .filter(|parent| parent.guid() == guid)
        .or_else(|| world.object_identity(guid))?;
    if world.object_identity(guid) != Some(parent)
        || !matches!(
            world.object_kind(guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        )
    {
        return None;
    }
    Some(UnitPassengerModelInput {
        parent,
        parent_live: true,
        seat: world
            .unit_vehicle(guid)
            .and_then(|vehicle| vehicles.passenger_seat(vehicle.definition_id(), slot)),
        parent_pose: world
            .object_transform(guid)
            .unwrap_or(WorldTransform::new(Vec3::ZERO, 0.)),
        parent_frame: frames
            .current_frame(parent)
            .map_or(Mat4::IDENTITY, |frame| frame.world_matrix()),
        parent_velocity: frames.velocity(parent),
    })
}

fn refresh_input(
    world: &ActiveWorld,
    frames: &UnitPassengerFrames,
    input: &mut Option<UnitPassengerModelInput>,
) {
    if let Some(input) = input {
        input.parent_live = world.object_identity(input.parent.guid()) == Some(input.parent);
        if input.parent_live
            && let Some(pose) = world.object_transform(input.parent.guid())
        {
            input.parent_pose = pose;
            if let Some(frame) = frames.current_frame(input.parent) {
                input.parent_frame = frame.world_matrix();
            }
            input.parent_velocity = frames.velocity(input.parent);
        }
    }
}

impl UnitAnimationBehavior {
    pub fn synchronize_passenger(
        &self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        frames: &UnitPassengerFrames,
        admitted_parent: Option<WorldObjectIdentity>,
        now_ms: u32,
    ) {
        self.passenger.borrow_mut().synchronize(
            world,
            vehicles,
            frames,
            self.identity,
            admitted_parent,
            now_ms,
        );
    }

    pub fn passenger_phase(&self) -> Phase {
        self.passenger.borrow().phase
    }

    pub fn passenger_pose_input(&self) -> Option<UnitPassengerModelInput> {
        let state = self.passenger.borrow();
        match state.phase {
            Phase::Seated => state.input,
            Phase::EnterDelay | Phase::ExitDelay => state.old_input,
            _ => None,
        }
    }

    pub fn advance_passenger(&self, now_ms: u32, target: Vec3, parent_velocity: Vec3) {
        let mut state = self.passenger.borrow_mut();
        state.advance(now_ms, target, parent_velocity);
    }

    pub fn passenger_needs_advance(&self, now_ms: u32) -> bool {
        let state = self.passenger.borrow();
        !matches!(state.phase, Phase::Detached | Phase::Seated)
            && state.timing.is_none_or(|timing| timing.finished(now_ms))
    }

    pub fn take_passenger_animation_change(&self) -> bool {
        std::mem::take(&mut self.passenger.borrow_mut().animation_changed)
    }

    pub fn passenger_animation_input(&self) -> Option<VehiclePassengerAnimationInput> {
        let state = self.passenger.borrow();
        let seat = state.input?.seat?;
        Some(VehiclePassengerAnimationInput {
            phase: state.phase,
            flags: seat.flags(),
            completed: state.animation_completed,
            special_exit: state.special_exit,
            enter: seat.enter_animations(),
            seated: seat.seated_animations(),
            secondary: seat.secondary_animations(),
            exit: seat.exit_animations(),
        })
    }

    /// 748560 calls 747B20 before ordinary locomotion in a delay/travel phase.
    pub fn passenger_transition_animation(&self) -> Option<u16> {
        self.passenger_animation_input()?
            .before_movement(self.input.get().alive)
            .and_then(|animation| u16::try_from(animation).ok())
    }

    pub fn complete_passenger_animation(&self, key: i32) {
        if let Some(input) = self.passenger_animation_input() {
            self.passenger.borrow_mut().animation_completed = input.complete(key);
        }
    }

    /// 748770 clears both bits again after the phase's initial animation call,
    /// which can interrupt a timer and invoke 7484E0 synchronously.
    pub fn finish_passenger_animation_change(&self) {
        let mut state = self.passenger.borrow_mut();
        if std::mem::take(&mut state.animation_reset_pending) {
            state.animation_completed = 0;
        }
    }

    pub fn passenger_transition_transform(
        &self,
        now_ms: u32,
        target: Vec3,
        scale: f32,
    ) -> Option<Mat4> {
        let mut state = self.passenger.borrow_mut();
        let pose = state.unit_pose?;
        let flags = state
            .input
            .and_then(|input| input.seat)
            .map_or(0, |seat| seat.flags());
        let origin = state
            .origin_parent
            .and_then(|(parent, offset)| {
                [state.input, state.old_input]
                    .into_iter()
                    .flatten()
                    .find(|input| input.parent == parent && input.parent_live)
                    .map(|input| input.parent_pose.position() + offset)
            })
            .unwrap_or(state.origin);
        let target = if state.phase == Phase::Entering {
            target
        } else {
            pose.position()
        };
        let sample =
            state
                .timing
                .as_mut()?
                .sample(now_ms, flags, origin, target, pose.orientation());
        state.last_yaw = Some(sample.yaw);
        let (sine, cosine) = (
            f64::from(sample.yaw).sin() as f32,
            f64::from(sample.yaw).cos() as f32,
        );
        Some(Mat4::from_cols(
            Vec3::new(cosine * scale, sine * scale, 0.).extend(0.),
            Vec3::new(-sine * scale, cosine * scale, 0.).extend(0.),
            Vec3::new(0., 0., scale).extend(0.),
            sample.position.extend(1.),
        ))
    }

    pub fn passenger_input(&self) -> Option<UnitPassengerModelInput> {
        self.passenger.borrow().input
    }

    pub fn passenger_last_transform(&self) -> Option<Mat4> {
        self.passenger.borrow().last_transform
    }

    pub fn passenger_last_yaw(&self) -> Option<f32> {
        self.passenger.borrow().last_yaw
    }

    pub fn passenger_seat_yaw(&self, current: f32, inherit: bool) -> f32 {
        let state = self.passenger.borrow();
        if inherit && matches!(state.phase, Phase::EnterDelay | Phase::ExitDelay) {
            state.retained_seat_yaw
        } else {
            current
        }
    }

    /// 748400 captures the static anchor from GetModel, including a mount.
    pub fn passenger_anchor(&self, model: &DecodedM2Model) -> Option<Vec3> {
        let mut state = self.passenger.borrow_mut();
        if let Some(anchor) = state.anchor {
            return anchor;
        }
        let anchor = state
            .input
            .and_then(|input| input.seat)
            .and_then(|seat| model.attachment(seat.passenger_attachment_id() as u32))
            .map(|attachment| attachment.position());
        state.anchor = Some(anchor);
        anchor
    }

    pub fn publish_passenger_transform(&self, transform: Mat4) {
        let mut state = self.passenger.borrow_mut();
        state.last_transform = Some(transform);
        if matches!(state.phase, Phase::Detached | Phase::Seated) {
            state.last_yaw = Some(self.body_pose().placement_yaw);
        }
    }

    /// Read the already advanced clock without taking its completion/event window.
    pub fn scene_clock(&self) -> Option<M2AnimationClock> {
        self.scene_sample
            .borrow()
            .as_ref()
            .map(|sample| sample.advance.clock)
    }
}
