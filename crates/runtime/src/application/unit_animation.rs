//! Retained unit posture/movement requests and their primary sequence callback.

#[cfg(test)]
#[path = "../../tests/application/unit_animation.rs"]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use solarity_asset::{AnimationDataCatalog, DecodedM2Model, M2ModelAnimationMode};
use solarity_ecs::{ActiveWorld, UnitAnimationTier, WorldMovementState, WorldObjectIdentity};
use solarity_rendering::{M2EventTimeWindow, M2SequenceStartPhase};
use solarity_systems::{
    UnitLocomotionAnimation, UnitMovementAnimationDecision, UnitPrimaryAnimationCompletion,
    UnitStandAnimationDecision, resolve_unit_airborne_animation, resolve_unit_landing_animation,
    resolve_unit_locomotion_animation, resolve_unit_model_animation,
    resolve_unit_movement_animation_completion, resolve_unit_primary_animation_completion,
    resolve_unit_stand_animation, resolve_unit_stand_transition, resolve_unit_turn_animation,
    unit_movement_is_airborne,
};

use super::model_playback::{M2Playback, M2PlaybackAdvance};
use super::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnitAnimationInput {
    pub stand: u8,
    pub locomotion: UnitLocomotionAnimation,
    pub tier: UnitAnimationTier,
    pub movement_flags: u32,
    pub secondary_flags: u16,
    pub airborne: bool,
    pub mounted: bool,
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
            secondary_flags: 0,
            airborne: false,
        };
        movement.map_or(input, |movement| input.with_movement(movement))
    }

    pub fn with_movement(mut self, movement: WorldMovementState) -> Self {
        self.locomotion = resolve_unit_locomotion_animation(movement);
        self.movement_flags = movement.flags() as u32;
        self.secondary_flags = (movement.flags() >> 32) as u16;
        self.airborne = unit_movement_is_airborne(
            self.movement_flags,
            movement
                .context()
                .falling
                .map_or(0.0, |fall| fall.vertical_speed),
        );
        self
    }
}

#[derive(Clone, Copy)]
pub(super) enum UnitMovementAnimationEventKind {
    Changed,
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
}

impl UnitAnimationScene {
    pub fn clear(&mut self) {
        self.owners.clear();
    }

    pub fn retain_world(&mut self, world: &ActiveWorld) {
        self.owners
            .retain(|guid, owner| world.object_identity(*guid) == Some(owner.identity));
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
            self.owners.insert(
                identity.guid(),
                Rc::new(UnitAnimationBehavior::new(
                    identity,
                    Arc::clone(model),
                    Arc::clone(animations),
                    input,
                )),
            );
        }
    }

    pub fn get(&self, guid: u64) -> Option<&Rc<UnitAnimationBehavior>> {
        self.owners.get(&guid)
    }

    pub fn notify_movement(&self, event: UnitMovementAnimationEvent) {
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
    pending: RefCell<VecDeque<PendingUnitAnimation>>,
    landing: Cell<bool>,
    playback: Rc<RefCell<M2Playback>>,
    scene_sample: RefCell<Option<UnitAnimationSceneSample>>,
}

impl UnitAnimationBehavior {
    pub fn new(
        identity: WorldObjectIdentity,
        model: Arc<DecodedM2Model>,
        animations: Arc<AnimationDataCatalog>,
        input: UnitAnimationInput,
    ) -> Self {
        Self {
            identity,
            model,
            animations,
            input: Cell::new(input),
            processed_stand: Cell::new(0),
            pending: RefCell::new(VecDeque::from([PendingUnitAnimation {
                input,
                event: UnitMovementAnimationEventKind::Changed,
            }])),
            landing: Cell::new(false),
            playback: Rc::new(RefCell::new(M2Playback::unstarted(0))),
            scene_sample: RefCell::new(None),
        }
    }

    pub fn matches(&self, identity: WorldObjectIdentity, model: &DecodedM2Model) -> bool {
        self.identity == identity && self.model.path() == model.path()
    }

    pub fn set_input(&self, input: UnitAnimationInput) {
        if self.input.replace(input) != input {
            self.pending.borrow_mut().push_back(PendingUnitAnimation {
                input,
                event: UnitMovementAnimationEventKind::Changed,
            });
        }
    }

    fn notify_movement(&self, event: UnitMovementAnimationEvent) {
        let mut input = self.input.get().with_movement(event.movement);
        input.stand = event.stand;
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

    fn behavior(&self, playback: &M2Playback) -> u16 {
        self.animations
            .definition(u32::from(playback.animation_id))
            .and_then(|definition| u16::try_from(definition.behavior_id()).ok())
            .unwrap_or(506) // Unit_C's missing AnimationData behavior sentinel.
    }

    fn request(&self, input: UnitAnimationInput, playback: &M2Playback) -> Option<u16> {
        // A death posture consumes locomotion changes while the primary dies.
        if input.stand == 7 {
            return None;
        }
        if input.mounted {
            return Some(UnitLocomotionAnimation::MOUNT.animation_id());
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
            0,
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
            UnitStandAnimationDecision::Continue => Some(input.locomotion.animation_id()),
            UnitStandAnimationDecision::Retain => None,
            UnitStandAnimationDecision::Select(animation) => Some(animation),
        }
    }

    fn transition_request(&self, input: UnitAnimationInput, playback: &M2Playback) -> Option<u16> {
        // 73F060's changed-stand path precedes the general 724500 resolver.
        if input.stand != self.processed_stand.get() {
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
        // 737EF0 leaves an identical primary and its variation roll untouched.
        if request.variation.is_none()
            && playback.script_timer.is_some()
            && playback.animation_id == animation_id
        {
            return Ok(());
        }
        playback.apply_resolved_model_sequence_variation(
            &self.model,
            animation_id,
            request.variation,
            mode,
            0,
            scene_time_ms,
            phase,
            true,
            random,
        )?;
        self.landing.set(self.behavior(playback) == 39);
        Ok(())
    }

    pub fn synchronize(
        &self,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut playback = self.playback.borrow_mut();
        loop {
            let Some(pending) = self.pending.borrow().front().copied() else {
                break;
            };
            let input = pending.input;
            let request = match pending.event {
                UnitMovementAnimationEventKind::Jump if input.stand != 7 && !input.mounted => {
                    self.landing.set(false);
                    Some(37)
                }
                UnitMovementAnimationEventKind::Land {
                    previous_flags,
                    forced,
                    slow,
                } if input.stand != 7 && !input.mounted => {
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
            self.processed_stand.set(input.stand);
            self.pending.borrow_mut().pop_front();
        }
        Ok(())
    }

    pub fn advance_scene(
        &self,
        scene_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.synchronize(scene_time_ms as u32, random)?;
        let mut playback = self.playback.borrow_mut();
        let mut completed = |playback: &mut M2Playback, random: &mut CrtRand| {
            let input = self.input.get();
            let behavior = self.behavior(playback);
            if matches!(behavior, 39 | 187) {
                self.landing.set(false);
            }
            let request = if !input.mounted || input.stand == 7 {
                let movement_completion =
                    resolve_unit_movement_animation_completion(behavior, input.movement_flags);
                let completion =
                    if let UnitMovementAnimationDecision::Select(animation) = movement_completion {
                        UnitPrimaryAnimationCompletion::Select(animation)
                    } else {
                        resolve_unit_primary_animation_completion(
                            behavior,
                            input.stand,
                            input.stand == 7,
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
        let advance = playback.clock_with_completion(
            &self.model,
            scene_time_ms,
            global_time_ms,
            random,
            Some(&mut completed),
        )?;
        let event_window = playback.event_window(scene_time_ms, global_time_ms);
        *self.scene_sample.borrow_mut() = Some(UnitAnimationSceneSample {
            advance,
            event_window,
        });
        Ok(())
    }
}
