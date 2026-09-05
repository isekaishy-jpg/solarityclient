//! Retained generic GameObject state, independent of GPU placement lifetime.

#[cfg(test)]
#[path = "../../tests/application/game_object_behavior.rs"]
mod tests;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use solarity_asset::{AnimationDataCatalog, DecodedM2Model, M2ModelAnimationMode};
use solarity_ecs::{ActiveWorld, GameObjectPresentation, WorldObjectIdentity, WorldStateError};
use solarity_rendering::{M2EventTimeWindow, M2SequenceStartPhase};
use solarity_systems::{
    GameObjectAnimationRequest, GameObjectAnimationState, GameObjectPlacement, PlacedM2Collision,
    game_object_reversed_progress, game_object_sequence_offset,
};

use crate::application::model_playback::{M2Playback, M2PlaybackAdvance};
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

/// Ordered GameObject handlers from the native mirror notification pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) enum GameObjectNotification {
    Initialize,
    Flags { previous: u32 },
    Progress,
    State,
}

/// The behavior owns its current internal state and shares one model timer.
pub(in crate::application) struct GameObjectBehavior {
    identity: WorldObjectIdentity,
    animations: Arc<AnimationDataCatalog>,
    cached_state: Cell<u8>,
    state: Cell<Option<GameObjectAnimationState>>,
    door: bool,
    collision_enabled: Cell<bool>,
    current_request: Cell<Option<u16>>,
    model: RefCell<Option<GameObjectModel>>,
    scene_sample: RefCell<Option<GameObjectSceneSample>>,
}

pub(in crate::application) struct GameObjectSceneSample {
    pub advance: M2PlaybackAdvance,
    pub event_window: M2EventTimeWindow,
}

struct GameObjectModel {
    display_id: u32,
    model: Arc<DecodedM2Model>,
    playback: Rc<RefCell<M2Playback>>,
    collision: Option<PlacedM2Collision>,
    collision_initialized: bool,
}

impl GameObjectBehavior {
    pub(in crate::application) fn new(
        identity: WorldObjectIdentity,
        presentation: GameObjectPresentation,
        animations: Arc<AnimationDataCatalog>,
    ) -> Self {
        Self {
            identity,
            animations,
            cached_state: Cell::new(presentation.state()),
            state: Cell::new(GameObjectAnimationState::initial(
                presentation.state(),
                presentation.sequence_progress(),
            )),
            door: presentation.object_type() == 0,
            collision_enabled: Cell::new(false),
            current_request: Cell::new(None),
            model: RefCell::new(None),
            scene_sample: RefCell::new(None),
        }
    }

    pub(in crate::application) fn state(&self) -> Option<GameObjectAnimationState> {
        self.state.get()
    }

    pub(in crate::application) fn collision_eligible(
        &self,
        object_type: u8,
        query_flags: u32,
    ) -> bool {
        !(object_type == 0 && query_flags & 0x8000 != 0) && self.collision_enabled.get()
    }

    pub(in crate::application) fn playback(&self) -> Option<Rc<RefCell<M2Playback>>> {
        self.model
            .borrow()
            .as_ref()
            .map(|model| Rc::clone(&model.playback))
    }

    /// CPU completion starts playback even when GPU preparation or placement is pending.
    pub(in crate::application) fn attach_model(
        &self,
        world: &ActiveWorld,
        display_id: u32,
        model: &Arc<DecodedM2Model>,
        placement: Option<GameObjectPlacement>,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.model.borrow().as_ref().is_some_and(|current| {
            current.display_id == display_id && Arc::ptr_eq(&current.model, model)
        }) {
            return self.synchronize_collision(placement);
        }
        let mut playback = M2Playback::unstarted(0);
        playback.scene_time_ms = scene_time_ms;
        playback.previous_event_scene_time_ms = scene_time_ms;
        self.current_request.set(None);
        self.reselect(
            world,
            model,
            &mut playback,
            scene_time_ms,
            M2SequenceStartPhase::BeforeSceneUpdate,
            random,
        )?;
        *self.model.borrow_mut() = Some(GameObjectModel {
            display_id,
            model: Arc::clone(model),
            playback: Rc::new(RefCell::new(playback)),
            collision: None,
            collision_initialized: false,
        });
        self.synchronize_collision(placement)
    }

    fn synchronize_collision(
        &self,
        placement: Option<GameObjectPlacement>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut model = self.model.borrow_mut();
        let Some(model) = model.as_mut() else {
            return Ok(());
        };
        let Some(placement) = placement else {
            model.collision.take();
            return Ok(());
        };
        if let Some(collision) = &mut model.collision {
            collision.set_transform(placement.matrix())?;
        } else {
            model.collision = Some(PlacedM2Collision::prepare_transform(
                Arc::clone(&model.model),
                placement.matrix(),
            )?);
        }
        if !model.collision_initialized {
            if let Some(collision) = &model.collision {
                self.initialize_collision(collision.collision_bounds());
            }
            model.collision_initialized = true;
        }
        Ok(())
    }

    fn initialize_collision(&self, bounds: solarity_systems::MovementCollisionBounds) {
        // 712F30 / 713F50 write +20C only when behavior virtual +4
        // allows it. Later 70CBE0 matrix updates never rewrite that flag.
        if !self.door || self.state.get() == Some(GameObjectAnimationState::Closed) {
            self.collision_enabled.set(bounds.has_positive_extent());
        }
    }

    pub(in crate::application) fn detach_model(&self) {
        self.model.borrow_mut().take();
        self.scene_sample.borrow_mut().take();
        self.current_request.set(None);
    }

    /// Scene callbacks run for every loaded model, including unresolved placements.
    pub(in crate::application) fn advance_scene(
        &self,
        world: &ActiveWorld,
        scene_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.scene_sample.borrow_mut().take();
        let model = self.model.borrow();
        let Some(model) = model.as_ref() else {
            return Ok(());
        };
        let mut playback = model.playback.borrow_mut();
        let advance = self.advance(
            world,
            &model.model,
            &mut playback,
            scene_time_ms,
            global_time_ms,
            random,
        )?;
        let event_window = playback.event_window(scene_time_ms, global_time_ms);
        *self.scene_sample.borrow_mut() = Some(GameObjectSceneSample {
            advance,
            event_window,
        });
        Ok(())
    }

    pub(in crate::application) fn take_scene_sample(&self) -> Option<GameObjectSceneSample> {
        self.scene_sample.borrow_mut().take()
    }

    /// Applies a notification against live fields, after all raw words were admitted.
    pub(in crate::application) fn notify(
        &self,
        world: &ActiveWorld,
        notification: GameObjectNotification,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let fields = self.fields(world)?;
        if self
            .model
            .borrow()
            .as_ref()
            .is_some_and(|model| model.display_id != fields.display_id())
        {
            self.detach_model();
        }
        let model = self.model.borrow();
        let mut playback = model.as_ref().map(|model| model.playback.borrow_mut());
        match notification {
            GameObjectNotification::Initialize => {}
            GameObjectNotification::Flags { previous } => {
                if (fields.flags() ^ previous) & 0x80 != 0
                    && self
                        .state
                        .get()
                        .is_some_and(GameObjectAnimationState::is_transition)
                    && let Some(playback) = playback.as_mut()
                {
                    playback.set_paused(fields.flags() & 0x80 != 0, scene_time_ms);
                }
            }
            GameObjectNotification::Progress => {
                if let (Some(model), Some(playback)) = (model.as_ref(), playback.as_mut()) {
                    self.reselect(
                        world,
                        &model.model,
                        playback,
                        scene_time_ms,
                        M2SequenceStartPhase::BeforeSceneUpdate,
                        random,
                    )?;
                }
            }
            GameObjectNotification::State => {
                let previous = self.cached_state.replace(fields.state());
                if previous != fields.state() || fields.object_type() == 15 {
                    let next = GameObjectAnimationState::changed(
                        previous,
                        fields.state(),
                        fields.sequence_progress(),
                    );
                    if let (Some(model), Some(playback)) = (model.as_ref(), playback.as_mut()) {
                        self.change_state(
                            world,
                            &model.model,
                            playback,
                            next,
                            scene_time_ms,
                            M2SequenceStartPhase::BeforeSceneUpdate,
                            random,
                        )?;
                    } else {
                        self.set_state(next);
                    }
                }
            }
        }
        Ok(())
    }

    /// The callback mutates the same state later read by collision eligibility.
    pub(in crate::application) fn advance(
        &self,
        world: &ActiveWorld,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        scene_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        playback.clock_with_completion(
            model,
            scene_time_ms,
            global_time_ms,
            random,
            Some(&mut |playback, random| {
                let Some(current) = self.state.get() else {
                    return Ok(());
                };
                let fields = self.fields(world)?;
                let next = current.completed(self.cached_state.get(), fields.sequence_progress());
                if next == Some(current) {
                    self.reselect(
                        world,
                        model,
                        playback,
                        playback.scene_time_ms,
                        M2SequenceStartPhase::DuringSceneUpdate,
                        random,
                    )
                } else {
                    self.change_state(
                        world,
                        model,
                        playback,
                        next,
                        playback.scene_time_ms,
                        M2SequenceStartPhase::DuringSceneUpdate,
                        random,
                    )
                }
            }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn change_state(
        &self,
        world: &ActiveWorld,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        next: Option<GameObjectAnimationState>,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let previous = self.state.get();
        if next == previous {
            self.set_state(next);
            return Ok(());
        }
        if let (Some(next), Some(previous), Some(timer)) = (next, previous, playback.script_timer)
            && next.reverses(previous)
            && self.fields(world)?.sequence_progress().is_none()
            && let Some(progress) = game_object_reversed_progress(
                scene_time_ms,
                timer.start_time_ms(),
                timer.end_time_ms(),
            )
        {
            world.set_game_object_sequence_progress(self.identity.guid(), progress)?;
        }
        self.state.set(next);
        self.reselect(world, model, playback, scene_time_ms, phase, random)?;
        self.update_door_collision();
        Ok(())
    }

    fn set_state(&self, next: Option<GameObjectAnimationState>) {
        self.state.set(next);
        self.update_door_collision();
    }

    fn update_door_collision(&self) {
        // Door virtual 70D8D0 writes after the generic setter, including its
        // unchanged-state early return, without checking model or bounds.
        if self.door {
            self.collision_enabled
                .set(self.state.get() == Some(GameObjectAnimationState::Closed));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn reselect(
        &self,
        world: &ActiveWorld,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(state) = self.state.get() else {
            return Ok(());
        };
        let animations = model.animations();
        let request = GameObjectAnimationRequest::resolve(animations, state.animation_id());
        if self
            .current_request
            .get()
            .is_some_and(|current| request.preserves_current(current))
        {
            return Ok(());
        }
        let fields = self.fields(world)?;
        let offset = if state.is_transition() {
            world
                .consume_game_object_sequence_progress(self.identity.guid())?
                .map_or(0, |progress| {
                    game_object_sequence_offset(
                        animations
                            .model_animation_duration_ms(
                                &self.animations,
                                u32::from(request.animation_id()),
                            )
                            .unwrap_or(0),
                        progress,
                    )
                })
        } else {
            0
        };
        if !animations.bones().is_empty()
            && let Some(resolved) = animations
                .resolve_model_animation(&self.animations, u32::from(request.animation_id()))
        {
            let mode = if request.frozen() {
                M2ModelAnimationMode::HoldStart
            } else {
                resolved.mode()
            };
            if playback.apply_resolved_model_sequence(
                model,
                resolved.animation_id(),
                mode,
                offset,
                scene_time_ms,
                phase,
                state != GameObjectAnimationState::Spawn,
                random,
            )? {
                self.current_request.set(Some(request.animation_id()));
            }
        }
        playback.set_paused(
            state.is_transition() && fields.flags() & 0x80 != 0,
            scene_time_ms,
        );
        Ok(())
    }

    fn fields(&self, world: &ActiveWorld) -> Result<GameObjectPresentation, WorldStateError> {
        if world.object_identity(self.identity.guid()) != Some(self.identity) {
            return Err(WorldStateError::UnknownObject {
                guid: self.identity.guid(),
            });
        }
        world.game_object_presentation(self.identity.guid()).ok_or(
            WorldStateError::MissingGameObjectPresentation {
                guid: self.identity.guid(),
            },
        )
    }
}
