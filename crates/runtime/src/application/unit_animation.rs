//! Retained unit posture/movement requests and their primary sequence callback.

mod ground;
mod mount;
pub(super) mod passenger;

use ground::UnitGroundPose;
pub(super) use mount::select_mount_animation;

#[cfg(test)]
#[path = "../../tests/application/unit_animation.rs"]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use glam::Mat4;
use solarity_asset::{AnimationDataCatalog, DecodedM2Model, M2ModelAnimationMode};
use solarity_ecs::{
    ActiveWorld, UnitAnimationTier, WorldMovementState, WorldObjectIdentity, WorldTransform,
};
use solarity_rendering::{
    M2AnimationClock, M2EventTimeWindow, M2ModelSequenceBlend, M2ModelSequenceTimer,
    M2SequenceStartPhase,
};
use solarity_systems::{
    UnitBodyOrientation, UnitBodyOrientationInput, UnitLocomotionAnimation,
    UnitMovementAnimationDecision, UnitPrimaryAnimationCompletion, UnitStandAnimationDecision,
    UnitWoundAnimationInput, resolve_unit_airborne_animation, resolve_unit_landing_animation,
    resolve_unit_locomotion_animation, resolve_unit_model_animation,
    resolve_unit_movement_animation_completion, resolve_unit_movement_speed,
    resolve_unit_primary_animation_completion, resolve_unit_stand_animation,
    resolve_unit_stand_transition, resolve_unit_turn_animation, resolve_unit_wound_animation,
    unit_movement_is_airborne,
};

use super::entity_opacity::EntityOpacityOwner;
use super::model_playback::{M2BoneEventCallback, M2Playback, M2PlaybackAdvance};
use super::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct UnitAnimationInput {
    pub stand: u8,
    pub locomotion: UnitLocomotionAnimation,
    pub tier: UnitAnimationTier,
    pub movement_flags: u32,
    pub movement_speed: f32,
    pub secondary_flags: u16,
    pub airborne: bool,
    pub mounted: bool,
    pub facing: f32,
    pub turn_rate: f32,
    pub alive: bool,
    pub feigning_death: bool,
    pub controlled: bool,
    pub mouse_turning: bool,
    pub attack_target_guid: u64,
    pub movement_spline_flags: u32,
}

impl UnitAnimationInput {
    pub fn new(
        stand: u8,
        tier: UnitAnimationTier,
        mounted: bool,
        movement: Option<WorldMovementState>,
    ) -> Self {
        let input = Self {
            stand,
            tier,
            mounted,
            locomotion: UnitLocomotionAnimation::STAND,
            movement_flags: 0,
            movement_speed: 0.0,
            secondary_flags: 0,
            airborne: false,
            facing: 0.0,
            turn_rate: std::f32::consts::PI,
            alive: stand != 7,
            feigning_death: false,
            controlled: false,
            mouse_turning: false,
            attack_target_guid: 0,
            movement_spline_flags: 0,
        };
        movement.map_or(input, |movement| input.with_movement(movement))
    }

    pub fn with_movement(mut self, movement: WorldMovementState) -> Self {
        self.locomotion = resolve_unit_locomotion_animation(movement);
        self.movement_flags = movement.flags() as u32;
        self.movement_speed = resolve_unit_movement_speed(movement);
        self.secondary_flags = (movement.flags() >> 32) as u16;
        self.movement_spline_flags = movement.spline().map_or(0, |spline| spline.flags);
        self.turn_rate = movement.speeds().turn_rate();
        self.airborne = unit_movement_is_airborne(
            self.movement_flags,
            movement
                .context()
                .falling
                .map_or(0.0, |fall| fall.vertical_speed),
        ) || movement.spline().is_some_and(|spline| spline.is_airborne());
        self
    }

    pub fn with_orientation(
        mut self,
        world: &ActiveWorld,
        guid: u64,
        controlled: bool,
        mouse_turning: bool,
    ) -> Self {
        self.facing = world
            .object_transform(guid)
            .map_or(0.0, |value| value.orientation());
        self.alive = world
            .unit_vitals(guid)
            .is_some_and(|value| (value.health() as i32) > 0);
        self.feigning_death = world
            .unit_flags(guid)
            .is_some_and(|flags| flags.secondary() & 1 != 0);
        self.attack_target_guid = world.unit_attack_target(guid);
        self.controlled = controlled;
        self.mouse_turning = mouse_turning;
        self
    }

    fn direct_facing(self) -> bool {
        self.movement_flags & 0x30 != 0 || self.mouse_turning
    }

    /// 71F560: replicated signed health, feign death and dead posture.
    fn dead(self) -> bool {
        !self.alive || self.feigning_death || self.stand == 7
    }

    fn same_primary_request(self, other: Self) -> bool {
        self.stand == other.stand
            && self.locomotion == other.locomotion
            && self.tier == other.tier
            && self.movement_flags == other.movement_flags
            && self.movement_speed == other.movement_speed
            && self.secondary_flags == other.secondary_flags
            && self.airborne == other.airborne
            && self.mounted == other.mounted
            && self.alive == other.alive
            && self.dead() == other.dead()
    }
}

#[derive(Clone, Copy)]
pub(super) enum UnitMovementAnimationEventKind {
    Changed,
    /// Frozen when a parent/seat change executes, before model residency.
    Passenger {
        previous_transform: WorldTransform,
        previous: Option<(u64, i8)>,
        parent: Option<WorldObjectIdentity>,
        animated: bool,
    },
    VisualKit {
        animation: u16,
        attack_target_guid: u64,
        template_flags: Option<u32>,
    },
    Jump,
    Land {
        previous_flags: u32,
        forced: bool,
        slow: bool,
    },
}

/// Frozen at movement execution, independently of encrypted writer admission.
#[derive(Clone, Copy)]
pub(super) struct UnitMovementAnimationEvent {
    pub identity: WorldObjectIdentity,
    pub movement: WorldMovementState,
    pub stand: u8,
    pub kind: UnitMovementAnimationEventKind,
}

#[derive(Clone, Copy)]
struct PendingUnitAnimation {
    input: UnitAnimationInput,
    event: UnitMovementAnimationEventKind,
}

/// Playback lifetimes survive presentation and GPU resource replacement.
#[derive(Default)]
pub(super) struct UnitAnimationScene {
    owners: BTreeMap<u64, Rc<UnitAnimationBehavior>>,
    /// Movement can publish before a model finishes loading for this unit.
    ground_poses: RefCell<BTreeMap<u64, (WorldObjectIdentity, Rc<UnitGroundPose>)>>,
    passenger_states:
        RefCell<BTreeMap<u64, (WorldObjectIdentity, passenger::SharedPassengerState)>>,
    scene_time_ms: u32,
}

impl UnitAnimationScene {
    pub fn clear(&mut self) {
        self.owners.clear();
        self.ground_poses.get_mut().clear();
        self.passenger_states.get_mut().clear();
        self.scene_time_ms = 0;
    }

    pub fn set_scene_time(&mut self, scene_time_ms: u32) {
        self.scene_time_ms = scene_time_ms;
    }

    pub fn scene_time_ms(&self) -> u32 {
        self.scene_time_ms
    }

    pub fn retain_world(&mut self, world: &ActiveWorld) {
        self.owners.retain(|guid, owner| {
            let keep = world.object_identity(*guid) == Some(owner.identity);
            if !keep {
                owner.opacity.mark_removed(self.scene_time_ms);
            }
            keep
        });
        self.ground_poses
            .get_mut()
            .retain(|guid, (identity, _)| world.object_identity(*guid) == Some(*identity));
        self.passenger_states
            .get_mut()
            .retain(|guid, (identity, _)| world.object_identity(*guid) == Some(*identity));
    }

    pub fn bind(
        &mut self,
        identity: WorldObjectIdentity,
        model: &Arc<DecodedM2Model>,
        animations: &Arc<AnimationDataCatalog>,
        input: UnitAnimationInput,
    ) {
        if let Some(owner) = self.owners.get(&identity.guid())
            && owner.matches(identity, model)
        {
            owner.set_input(input);
        } else {
            let mut replacement = UnitAnimationBehavior::new(
                identity,
                Arc::clone(model),
                Arc::clone(animations),
                input,
                self.scene_time_ms,
            );
            // 7197D0's normal exists before and across CM2Model replacements.
            replacement.ground = self.ground_pose(identity);
            replacement.passenger = self.passenger_state(identity);
            if let Some(previous) = self.owners.get(&identity.guid())
                && previous.identity == identity
            {
                replacement.opacity = Rc::clone(&previous.opacity);
            }
            self.owners.insert(identity.guid(), Rc::new(replacement));
        }
    }

    pub fn get(&self, guid: u64) -> Option<&Rc<UnitAnimationBehavior>> {
        self.owners.get(&guid)
    }

    pub fn notify_movement(&self, event: UnitMovementAnimationEvent) {
        if matches!(event.kind, UnitMovementAnimationEventKind::Passenger { .. }) {
            self.passenger_state(event.identity)
                .borrow_mut()
                .queue(event);
            return;
        }
        if let Some(owner) = self.owners.get(&event.identity.guid())
            && owner.identity == event.identity
        {
            owner.notify_movement(event);
        }
    }
}

pub(super) struct UnitAnimationSceneSample {
    pub advance: M2PlaybackAdvance,
    pub event_window: M2EventTimeWindow,
}

#[derive(Clone, Copy)]
struct UnitSequenceRequest {
    animation: u16,
    variation: Option<u16>,
    resolve_unit_tier: bool,
}

impl From<u16> for UnitSequenceRequest {
    fn from(animation: u16) -> Self {
        Self {
            animation,
            variation: None,
            resolve_unit_tier: true,
        }
    }
}

/// The unit owns playback across GPU and equipment/texture replacements.
pub(super) struct UnitAnimationBehavior {
    identity: WorldObjectIdentity,
    model: Arc<DecodedM2Model>,
    animations: Arc<AnimationDataCatalog>,
    input: Cell<UnitAnimationInput>,
    processed_stand: Cell<u8>,
    processed_alive: Cell<bool>,
    pending: RefCell<VecDeque<PendingUnitAnimation>>,
    landing: Cell<bool>,
    playback: Rc<RefCell<M2Playback>>,
    scene_sample: RefCell<Option<UnitAnimationSceneSample>>,
    body: RefCell<UnitBodyPose>,
    ground: Rc<UnitGroundPose>,
    model_color: Cell<u32>,
    opacity: Rc<EntityOpacityOwner>,
    passenger: Rc<RefCell<passenger::UnitPassengerModel>>,
}

/// Instance state survives GPU rebuilds alongside the primary sequence owner.
struct UnitBodyPose {
    controller: UnitBodyOrientation,
    last_scene_time: Option<f32>,
    facing_changed: bool,
    facing_tick: u32,
    sample: UnitBodyPoseSample,
}

#[derive(Clone, Copy)]
pub(super) struct UnitBodyPoseSample {
    pub placement_rotation: Mat4,
    /// Final native body heading, before terrain alignment and body scale.
    pub placement_yaw: f32,
    transforms: [(u16, Mat4); 2],
    transform_count: usize,
    procedural_turn: u32,
}

impl UnitBodyPoseSample {
    pub fn bone_transforms(&self) -> &[(u16, Mat4)] {
        &self.transforms[..self.transform_count]
    }
}

/// Model-side pose state survives detachment without retaining unit callbacks.
#[derive(Clone, Copy)]
pub(super) struct UnitRetiredPose {
    pub body: UnitBodyPoseSample,
    pub color: u32,
}

fn body_rotation(angle: f32) -> Mat4 {
    if angle == 0.0 {
        return Mat4::IDENTITY;
    }
    // 4C3460 uses x87 FSIN/FCOS before storing the matrix's float elements.
    let (sin, cos) = f64::from(angle).sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    Mat4::from_cols_array(&[
        cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

impl UnitAnimationBehavior {
    pub fn new(
        identity: WorldObjectIdentity,
        model: Arc<DecodedM2Model>,
        animations: Arc<AnimationDataCatalog>,
        input: UnitAnimationInput,
        scene_time_ms: u32,
    ) -> Self {
        Self {
            identity,
            model,
            animations,
            input: Cell::new(input),
            processed_stand: Cell::new(0),
            processed_alive: Cell::new(true),
            pending: RefCell::new(VecDeque::from([PendingUnitAnimation {
                input,
                event: UnitMovementAnimationEventKind::Changed,
            }])),
            landing: Cell::new(false),
            playback: Rc::new(RefCell::new(M2Playback::unstarted(0, scene_time_ms))),
            scene_sample: RefCell::new(None),
            model_color: Cell::new(u32::MAX),
            opacity: Rc::new(EntityOpacityOwner::default()),
            passenger: Rc::new(RefCell::new(passenger::UnitPassengerModel::default())),
            ground: Rc::new(UnitGroundPose::default()),
            body: RefCell::new(UnitBodyPose {
                controller: UnitBodyOrientation::new(input.facing),
                last_scene_time: None,
                facing_changed: true,
                facing_tick: 0,
                sample: UnitBodyPoseSample {
                    placement_rotation: Mat4::IDENTITY,
                    placement_yaw: input.facing,
                    transforms: [(4, Mat4::IDENTITY), (6, Mat4::IDENTITY)],
                    transform_count: 0,
                    procedural_turn: 0,
                },
            }),
        }
    }

    pub fn matches(&self, identity: WorldObjectIdentity, model: &DecodedM2Model) -> bool {
        self.identity == identity && self.model.path() == model.path()
    }

    pub fn identity(&self) -> WorldObjectIdentity {
        self.identity
    }

    pub fn set_model_color(&self, color: u32) {
        self.model_color.set(color);
    }
    pub fn model_color(&self) -> u32 {
        self.model_color.get()
    }

    pub fn opacity_owner(&self) -> &Rc<EntityOpacityOwner> {
        &self.opacity
    }

    /// 73B140 routes wound behaviors through 736640; other kits use 7385C0.
    #[cfg(test)]
    pub fn request_visual_kit_animation(&self, animation: u16) {
        self.request_environmental_animation(animation, self.input.get().attack_target_guid, None);
    }

    pub fn request_environmental_animation(
        &self,
        animation: u16,
        attack_target_guid: u64,
        template_flags: Option<u32>,
    ) {
        self.pending.borrow_mut().push_back(PendingUnitAnimation {
            input: self.input.get(),
            event: UnitMovementAnimationEventKind::VisualKit {
                animation,
                attack_target_guid,
                template_flags,
            },
        });
    }

    pub fn set_input(&self, input: UnitAnimationInput) {
        let previous = self.input.replace(input);
        if previous.facing != input.facing || previous.direct_facing() != input.direct_facing() {
            self.body.borrow_mut().facing_changed = true;
        }
        if !previous.same_primary_request(input) {
            self.pending.borrow_mut().push_back(PendingUnitAnimation {
                input,
                event: UnitMovementAnimationEventKind::Changed,
            });
        }
    }

    fn notify_movement(&self, event: UnitMovementAnimationEvent) {
        let mut input = self.input.get().with_movement(event.movement);
        input.stand = event.stand;
        if self.input.get().direct_facing() != input.direct_facing() {
            self.body.borrow_mut().facing_changed = true;
        }
        self.input.set(input);
        self.pending.borrow_mut().push_back(PendingUnitAnimation {
            input,
            event: event.kind,
        });
    }

    pub fn playback(&self) -> Rc<RefCell<M2Playback>> {
        Rc::clone(&self.playback)
    }

    pub fn take_scene_sample(&self) -> Option<UnitAnimationSceneSample> {
        self.scene_sample.borrow_mut().take()
    }

    pub fn body_pose(&self) -> UnitBodyPoseSample {
        self.body.borrow().sample
    }

    pub fn retirement_pose(&self) -> UnitRetiredPose {
        UnitRetiredPose {
            body: self.body_pose(),
            color: self.model_color(),
        }
    }

    fn advance_body(&self, scene_time_ms: f32) -> bool {
        let input = self.input.get();
        let mut body = self.body.borrow_mut();
        if body.last_scene_time == Some(scene_time_ms) {
            return false;
        }
        let frame_seconds = body.last_scene_time.map_or(0.0, |previous| {
            ((scene_time_ms - previous) * 0.001).max(0.0)
        });
        body.last_scene_time = Some(scene_time_ms);
        if body.facing_changed {
            body.facing_tick = scene_time_ms as u32;
            body.facing_changed = false;
        }
        let facing_elapsed_ms = (scene_time_ms as u32).wrapping_sub(body.facing_tick);
        let sample = body.controller.advance(UnitBodyOrientationInput {
            facing: input.facing,
            movement_flags: input.movement_flags,
            turn_rate: input.turn_rate,
            facing_elapsed_ms,
            frame_seconds,
            direct_facing: input.direct_facing(),
            full_spine_turn: input.controlled,
            has_spine: self.model.animations().key_bone(4).is_some(),
            has_head: self.model.animations().key_bone(6).is_some(),
            mounted: input.mounted,
            body_yaw_allowed: input.alive,
        });
        let changed = body.sample.procedural_turn != sample.procedural_turn;
        body.sample.procedural_turn = sample.procedural_turn;
        body.sample.placement_rotation = body_rotation(sample.yaw - input.facing);
        body.sample.placement_yaw = sample.yaw;
        body.sample.transform_count = 0;
        for (key, angle) in [(4, sample.spine), (6, sample.head)] {
            if let Some(angle) = angle {
                let index = body.sample.transform_count;
                body.sample.transforms[index] = (key, body_rotation(angle));
                body.sample.transform_count += 1;
            }
        }
        changed
    }

    fn behavior(&self, playback: &M2Playback) -> u16 {
        self.animation_behavior(playback.animation_id)
    }

    fn animation_behavior(&self, animation: u16) -> u16 {
        self.animations
            .definition(u32::from(animation))
            .and_then(|definition| u16::try_from(definition.behavior_id()).ok())
            .unwrap_or(506) // Unit_C's missing AnimationData behavior sentinel.
    }

    /// Read the model's retained bone timers, including an independent wound.
    pub fn bone_sequences(
        &self,
        clock: M2AnimationClock,
        scene_time_ms: u32,
    ) -> Option<Vec<(u16, M2AnimationClock)>> {
        let clocks = self
            .playback
            .borrow()
            .bone_sequence_clocks(&self.model, clock, scene_time_ms);
        (!clocks.is_empty()).then_some(clocks)
    }

    fn wound(
        &self,
        playback: &mut M2Playback,
        animation: u16,
        input: UnitAnimationInput,
        template_flags: Option<u32>,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) {
        let Some(wound) = resolve_unit_wound_animation(UnitWoundAnimationInput {
            critical: animation == 10,
            attack_target_guid: input.attack_target_guid,
            movement_flags: input.movement_flags,
            secondary_movement_flags: input.secondary_flags,
            movement_handler_flags: input.movement_spline_flags,
            stand: input.stand,
            mounted: input.mounted,
            primary_animation: playback.animation_id,
            primary_behavior: self.behavior(playback),
            current_animation: playback.animation_id,
            current_behavior: self.behavior(playback),
            upper_body_key_bone: [4, 6]
                .into_iter()
                .find(|key| self.model.animations().key_bone(*key).is_some()),
            dead: input.dead(),
            model_ready: !self.model.animations().sequences().is_empty(),
            template_flags,
            ..Default::default()
        }) else {
            return;
        };
        let Some(resolved) = resolve_unit_model_animation(
            &self.animations,
            UnitLocomotionAnimation::new(wound.animation),
            input.tier,
            |animation| {
                self.model
                    .animations()
                    .available_variation_count(animation)
                    .is_some()
            },
        ) else {
            return;
        };
        let Some(sequence) = self
            .model
            .animations()
            .select_model_sequence(resolved.animation_id(), random.next_u15())
        else {
            return;
        };
        if self.model.animations().is_sequence_available(sequence) != Some(true) {
            return;
        }
        let authored = &self.model.animations().sequences()[sequence];
        let timer = M2ModelSequenceTimer::new(
            authored,
            M2ModelAnimationMode::Forward,
            scene_time_ms,
            0,
            random.next_u15(),
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        let blend =
            M2ModelSequenceBlend::wound(sequence, timer, scene_time_ms, authored.duration_ms());
        if let Some(key) = wound
            .key_bone
            .filter(|key| self.model.animations().key_bone_lookup()[usize::from(*key)] != Some(0))
        {
            playback.set_bone_blend(&self.model, key, blend);
        } else {
            playback.script_blend = Some(blend);
        }
    }

    fn request(&self, input: UnitAnimationInput, playback: &M2Playback) -> Option<u16> {
        // A death posture consumes locomotion changes while the primary dies.
        if input.dead() {
            return None;
        }
        if input.mounted {
            return Some(UnitLocomotionAnimation::MOUNT.animation_id());
        }
        if let Some(animation) = self.passenger_transition_animation() {
            return Some(animation);
        }
        // 71DFF0 precedes ordinary movement/posture stages. 201's entry request
        // keeps ownership until its completion callback installs submerged 202.
        if input.stand == 9 {
            if self.behavior(playback) == 201 {
                return None;
            }
            if self
                .model
                .animations()
                .available_variation_count(202)
                .is_some()
            {
                return Some(202);
            }
        }
        if input.airborne {
            return match resolve_unit_airborne_animation(self.behavior(playback)) {
                UnitMovementAnimationDecision::Select(animation) => Some(animation),
                _ => None,
            };
        }
        // Movement precedes the posture stage in 724500's ordinary resolver.
        if input.locomotion != UnitLocomotionAnimation::STAND {
            return Some(input.locomotion.animation_id());
        }
        if let Some(turn) = resolve_unit_turn_animation(
            input.movement_flags,
            input.secondary_flags,
            self.body.borrow().sample.procedural_turn,
            self.landing.get(),
        ) {
            return Some(turn);
        }
        match resolve_unit_stand_animation(
            input.stand,
            self.processed_stand.get(),
            self.behavior(playback),
            input.movement_flags & 0x200000 != 0,
            false, // Death entry and its retained primary were handled above.
        ) {
            UnitStandAnimationDecision::Continue if self.landing.get() => None,
            UnitStandAnimationDecision::Continue => self
                .passenger_animation_input()
                .and_then(|passenger| passenger.after_movement())
                .and_then(|animation| u16::try_from(animation).ok())
                .or(Some(input.locomotion.animation_id())),
            UnitStandAnimationDecision::Retain => None,
            UnitStandAnimationDecision::Select(animation) => Some(animation),
        }
    }

    fn transition_request(&self, input: UnitAnimationInput, playback: &M2Playback) -> Option<u16> {
        // 73F330 -> 729220 -> 73AF80 is independent of the stand field.
        // 71DDE0 protects an already playing death/corpse family from restart.
        if (!input.alive && self.processed_alive.get())
            || (input.stand == 7 && self.processed_stand.get() != 7)
        {
            return if matches!(self.behavior(playback), 1 | 6 | 131 | 132 | 466..=468 | 472) {
                None
            } else {
                Some(if input.movement_flags & 0x200000 != 0 {
                    131
                } else {
                    466
                })
            };
        }
        if input.dead() {
            return None;
        }
        // 73F060's changed-stand path precedes the general 724500 resolver.
        // While mounted, 738B34 routes ordinary posture requests back to the
        // rider pose; 738CF3's full-body death override was handled above.
        if input.stand != self.processed_stand.get() && !input.mounted {
            match resolve_unit_stand_transition(
                input.stand,
                self.processed_stand.get(),
                self.behavior(playback),
                input.movement_flags & 0x200000 != 0,
                self.model
                    .animations()
                    .available_variation_count(127)
                    .is_some(),
            ) {
                UnitStandAnimationDecision::Continue => {}
                UnitStandAnimationDecision::Retain => return None,
                UnitStandAnimationDecision::Select(animation) => return Some(animation),
            }
        }
        self.request(input, playback)
    }

    #[allow(clippy::too_many_arguments)]
    fn select(
        &self,
        playback: &mut M2Playback,
        request: UnitSequenceRequest,
        input: UnitAnimationInput,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let death = matches!(
            self.animation_behavior(request.animation),
            1 | 6 | 131 | 132 | 466..=468 | 472
        );
        let key = self.upper_body_key();
        if death && let Some(key) = key {
            if playback
                .bone_playback(key)
                .is_some_and(M2Playback::has_pending_sequence_callback)
            {
                self.complete_passenger_animation(i32::from(key));
            }
            playback.clear_bone_blend(key);
            playback.clear_bone_sequence(&self.model, key, true, scene_time_ms);
        }
        let primary = (!input.dead() && !death)
            .then(|| self.passenger_animation_input()?.primary())
            .flatten()
            .and_then(|animation| u16::try_from(animation).ok());
        if let Some(primary) = primary {
            // 7385C0 copies a non-Stand ordinary request onto the upper slot,
            // then unconditionally forces the authored passenger body pose.
            // 737EF0 commits the upper request before the body request.
            if self.animation_behavior(request.animation) != 0
                && let Some(key) = key
            {
                self.commit_sequence(
                    playback,
                    request,
                    input,
                    scene_time_ms,
                    phase,
                    Some(key),
                    None,
                    random,
                )?;
            }
            self.commit_sequence(
                playback,
                UnitSequenceRequest {
                    animation: primary,
                    variation: None,
                    resolve_unit_tier: false,
                },
                input,
                scene_time_ms,
                phase,
                None,
                Some((1., 0)),
                random,
            )
        } else {
            self.commit_sequence(
                playback,
                request,
                input,
                scene_time_ms,
                phase,
                None,
                None,
                random,
            )
        }
    }

    fn upper_body_key(&self) -> Option<u16> {
        [4, 6]
            .into_iter()
            .find(|key| self.model.animations().key_bone(*key).is_some())
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_sequence(
        &self,
        playback: &mut M2Playback,
        request: UnitSequenceRequest,
        input: UnitAnimationInput,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        key: Option<u16>,
        forced_timing: Option<(f32, i32)>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let requested = request.animation;
        if self.model.animations().sequences().is_empty() {
            playback.animation_id = requested;
            return Ok(());
        }
        let (animation_id, mode) = if request.resolve_unit_tier {
            let animation = resolve_unit_model_animation(
                &self.animations,
                UnitLocomotionAnimation::new(requested),
                input.tier,
                |animation| {
                    self.model
                        .animations()
                        .available_variation_count(animation)
                        .is_some()
                },
            )
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: self.model.path().clone(),
                animation_id: requested,
            })?;
            (animation.animation_id(), M2ModelAnimationMode::Forward)
        } else {
            // 73B510 sends 6 and 132 directly to CM2Model. Its own fallback
            // can hold a missing corpse at the death clip's endpoint.
            let Some(animation) = self
                .model
                .animations()
                .resolve_model_animation(&self.animations, u32::from(requested))
            else {
                return Ok(());
            };
            (animation.animation_id(), animation.mode())
        };
        let (speed, offset) = forced_timing
            .unwrap_or_else(|| self.sequence_timing(playback, animation_id, input, scene_time_ms));
        let key = key.filter(|key| {
            self.model
                .animations()
                .key_bone_lookup()
                .get(usize::from(*key))
                != Some(&Some(0))
        });
        let previous = key.map_or(Some(&*playback), |key| playback.bone_playback(key));
        // 737EF0 leaves an identical primary and its variation roll untouched,
        // but changes its clock when the movement speed changes.
        let comparison_id = if request.resolve_unit_tier {
            animation_id
        } else {
            requested
        };
        if previous.is_some_and(|previous| {
            previous.script_timer.is_some() && previous.animation_id == comparison_id
        }) {
            if let Some(key) = key {
                playback.set_bone_sequence_speed(key, speed, scene_time_ms);
            } else {
                playback.set_sequence_speed(speed, scene_time_ms);
            }
            return Ok(());
        }
        // 8269C0 also reports interruption, including to the passenger bits.
        // Both body/upper requests were resolved before this synchronous call.
        if previous.is_some_and(M2Playback::has_pending_sequence_callback) {
            self.complete_passenger_animation(key.map_or(-1, i32::from));
        }
        if let Some(key) = key {
            playback.apply_bone_sequence(
                &self.model,
                key,
                animation_id,
                request.variation,
                mode,
                speed,
                offset,
                scene_time_ms,
                phase,
                random,
            )?;
            return Ok(());
        }
        playback.apply_resolved_model_sequence_variation(
            &self.model,
            animation_id,
            request.variation,
            mode,
            speed,
            offset,
            scene_time_ms,
            phase,
            true,
            random,
        )?;
        self.landing.set(self.behavior(playback) == 39);
        Ok(())
    }

    fn sequence_timing(
        &self,
        playback: &M2Playback,
        animation_id: u16,
        input: UnitAnimationInput,
        scene_time_ms: u32,
    ) -> (f32, i32) {
        model_sequence_timing(&self.model, playback, animation_id, input, scene_time_ms)
    }

    pub fn synchronize(
        &self,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.take_passenger_animation_change() {
            self.pending.borrow_mut().push_back(PendingUnitAnimation {
                input: self.input.get(),
                event: UnitMovementAnimationEventKind::Changed,
            });
        }
        let mut playback = self.playback.borrow_mut();
        loop {
            let Some(pending) = self.pending.borrow().front().copied() else {
                break;
            };
            let input = pending.input;
            let request = match pending.event {
                UnitMovementAnimationEventKind::VisualKit {
                    animation,
                    attack_target_guid,
                    template_flags,
                } if matches!(self.animation_behavior(animation), 8..=10) => {
                    self.wound(
                        &mut playback,
                        animation,
                        UnitAnimationInput {
                            attack_target_guid,
                            ..input
                        },
                        template_flags,
                        scene_time_ms,
                        random,
                    );
                    None
                }
                UnitMovementAnimationEventKind::VisualKit { animation, .. } => Some(animation),
                UnitMovementAnimationEventKind::Jump if !input.dead() && !input.mounted => {
                    self.landing.set(false);
                    Some(37)
                }
                UnitMovementAnimationEventKind::Land {
                    previous_flags,
                    forced,
                    slow,
                } if !input.dead() && !input.mounted => {
                    self.landing.set(false);
                    match resolve_unit_landing_animation(
                        previous_flags,
                        input.movement_flags,
                        forced,
                        slow,
                    ) {
                        UnitMovementAnimationDecision::Select(animation) => Some(animation),
                        UnitMovementAnimationDecision::Retain => None,
                        UnitMovementAnimationDecision::Continue => {
                            self.transition_request(input, &playback)
                        }
                    }
                }
                _ => self.transition_request(input, &playback),
            };
            if let Some(request) = request {
                self.select(
                    &mut playback,
                    request.into(),
                    input,
                    scene_time_ms,
                    M2SequenceStartPhase::BeforeSceneUpdate,
                    random,
                )?;
            }
            if matches!(pending.event, UnitMovementAnimationEventKind::Jump) {
                tracing::info!(
                    model = %self.model.path(),
                    scene_time_ms,
                    requested_animation = ?request,
                    animation_id = playback.animation_id,
                    sequence = playback.sequence,
                    variation = self.model.animations().sequences().get(playback.sequence).map(|sequence| sequence.variation_index()),
                    movement_flags = input.movement_flags,
                    "unit jump animation selection"
                );
            }
            self.processed_stand.set(input.stand);
            self.processed_alive.set(input.alive);
            self.pending.borrow_mut().pop_front();
        }
        self.finish_passenger_animation_change();
        Ok(())
    }

    #[cfg(test)]
    pub fn advance_scene(
        &self,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.prepare_scene(scene_time_ms, random)?;
        self.advance_prepared_scene(scene_time_ms, random, None)
    }

    /// Unit movement, yaw and pending selections precede CM2Scene's callbacks.
    /// World placement can be published between this step and sequence advance.
    pub fn prepare_scene(
        &self,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.scene_sample.borrow_mut().take();
        self.synchronize(scene_time_ms as u32, random)?;
        self.opacity
            .resolve_initial_animation(self.behavior(&self.playback.borrow()) == 127);
        self.opacity.advance(scene_time_ms as u32);
        let turn_changed = self.advance_body(scene_time_ms);
        let mut playback = self.playback.borrow_mut();
        if turn_changed {
            let input = self.input.get();
            // 73E3C2 rechecks 71DE90 even when catch-up has just stopped.
            // With no turn flags left it retains the turn clip until its
            // primary completion callback resolves the ordinary idle pose.
            if resolve_unit_turn_animation(
                input.movement_flags,
                input.secondary_flags,
                self.body.borrow().sample.procedural_turn,
                self.landing.get(),
            )
            .is_some()
                && let Some(request) = self.request(input, &playback)
            {
                self.select(
                    &mut playback,
                    request.into(),
                    input,
                    scene_time_ms as u32,
                    M2SequenceStartPhase::BeforeSceneUpdate,
                    random,
                )?;
            }
        }
        Ok(())
    }

    pub fn advance_prepared_scene(
        &self,
        scene_time_ms: f32,
        random: &mut CrtRand,
        event_callback: Option<&mut M2BoneEventCallback<'_>>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut playback = self.playback.borrow_mut();
        let mut completed =
            |playback: &mut M2Playback, key: i32, animation: u16, _: u32, random: &mut CrtRand| {
                let input = self.input.get();
                self.complete_passenger_animation(key);
                if !matches!(key, -1 | 26) {
                    let Ok(key) = u16::try_from(key) else {
                        return Ok(());
                    };
                    // 737BD0 retains a distinct seated body while re-resolving the
                    // ordinary/secondary request on the completed upper key.
                    let seated = !input.dead()
                        && self.passenger_animation_input().is_some_and(|passenger| {
                            passenger.phase == solarity_systems::VehiclePassengerPhase::Seated
                                && passenger.primary().is_some()
                        });
                    let request = self.request(input, playback);
                    if seated && request.is_none() {
                        return Ok(());
                    }
                    if seated
                        && let Some(request) =
                            request.filter(|request| *request != playback.animation_id)
                    {
                        return self.commit_sequence(
                            playback,
                            request.into(),
                            input,
                            playback.scene_time_ms,
                            M2SequenceStartPhase::DuringSceneUpdate,
                            Some(key),
                            Some((1., 0)),
                            random,
                        );
                    }
                    playback.clear_bone_sequence(&self.model, key, true, playback.scene_time_ms);
                    return Ok(());
                }
                let behavior = self.animation_behavior(animation);
                if matches!(behavior, 39 | 187) {
                    self.landing.set(false);
                }
                let passenger = (!input.dead() && !input.mounted)
                    .then(|| self.passenger_transition_animation())
                    .flatten();
                let seated_completed = !input.dead()
                    && self.passenger_animation_input().is_some_and(|passenger| {
                        passenger.phase == solarity_systems::VehiclePassengerPhase::Seated
                            && if passenger.primary().is_some() {
                                passenger.flags & 2 != 0
                                    && passenger.seated.contains(&i32::from(animation))
                            } else {
                                passenger.flags & 4 != 0
                                    && passenger.secondary.contains(&i32::from(animation))
                            }
                    });
                let request = if seated_completed {
                    Some(self.request(input, playback).unwrap_or(0).into())
                } else if let Some(animation) = passenger {
                    // 7385C0 reapplies the passenger primary after completion's
                    // ordinary request (for example jump-start -> jump-end).
                    Some(animation.into())
                } else if !input.mounted || input.dead() {
                    let movement_completion =
                        resolve_unit_movement_animation_completion(behavior, input.movement_flags);
                    let completion = if let UnitMovementAnimationDecision::Select(animation) =
                        movement_completion
                    {
                        UnitPrimaryAnimationCompletion::Select(animation)
                    } else {
                        resolve_unit_primary_animation_completion(
                            behavior,
                            input.stand,
                            input.dead(),
                            false,
                        )
                    };
                    match completion {
                        UnitPrimaryAnimationCompletion::Continue => {
                            self.request(input, playback).map(Into::into)
                        }
                        UnitPrimaryAnimationCompletion::Retain => None,
                        UnitPrimaryAnimationCompletion::Select(animation) => Some(animation.into()),
                        UnitPrimaryAnimationCompletion::SelectWithCurrentVariation(animation) => {
                            Some(UnitSequenceRequest {
                                animation,
                                variation: self
                                    .model
                                    .animations()
                                    .model_variation_ordinal(playback.sequence),
                                resolve_unit_tier: animation == 472,
                            })
                        }
                    }
                } else {
                    self.request(input, playback).map(Into::into)
                };
                if let Some(request) = request {
                    self.select(
                        playback,
                        request,
                        input,
                        playback.scene_time_ms,
                        M2SequenceStartPhase::DuringSceneUpdate,
                        random,
                    )?;
                }
                Ok(())
            };
        let advance = playback.clock_with_bone_callbacks(
            &self.model,
            scene_time_ms as u32,
            random,
            Some(&mut completed),
            event_callback,
        )?;
        let event_window = playback.event_window(scene_time_ms);
        *self.scene_sample.borrow_mut() = Some(UnitAnimationSceneSample {
            advance,
            event_window,
        });
        Ok(())
    }
}

/// 7385C0 queries the active model, which is the mount when one is present.
fn model_sequence_timing(
    model: &DecodedM2Model,
    playback: &M2Playback,
    animation_id: u16,
    input: UnitAnimationInput,
    scene_time_ms: u32,
) -> (f32, i32) {
    let animations = model.animations();
    // Ordinal zero precedes weighted selection. 714E80 tests the resolved ID,
    // not its AnimationData behavior.
    let Some(index) = animations.model_sequence_for_variation(animation_id, 0) else {
        return (1.0, 0);
    };
    let sequence = animations.sequences()[index];
    let previous = playback.script_timer.map(|timer| {
        let sequence = animations.sequences()[playback.sequence];
        (
            sequence.movement_speed(),
            sequence.duration_ms(),
            timer.unwrapped_time(scene_time_ms),
        )
    });
    unit_sequence_timing(
        animation_id,
        input.movement_flags,
        input.movement_speed,
        sequence.movement_speed(),
        sequence.duration_ms(),
        previous,
    )
}

/// Native `7388B4..73898F` speed and phase policy for a resolved unit sequence.
fn unit_sequence_timing(
    animation_id: u16,
    movement_flags: u32,
    movement_speed: f32,
    authored_speed: f32,
    duration: u32,
    previous: Option<(f32, u32, u32)>,
) -> (f32, i32) {
    if authored_speed == 0.0
        || movement_flags & 0xc0000f == 0
        || !matches!(
            animation_id,
            4 | 5 | 11 | 12 | 13 | 37 | 38 | 39 | 42 | 43 | 44 | 45 | 119 | 135 | 143 | 187 | 223
        )
    {
        return (1.0, 0);
    }
    let speed = (f64::from(movement_speed) / f64::from(authored_speed).abs()) as f32;
    let offset = match previous {
        Some((old_speed, old_duration, old_phase))
            if old_speed != 0.0 && old_duration != 0 && duration != 0 =>
        {
            // IMUL keeps the low 32 bits before both unsigned divisions.
            (old_phase.wrapping_mul(duration) / old_duration) % duration
        }
        _ => 0,
    };
    (speed, offset as i32)
}
