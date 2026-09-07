//! Primary CM2Model timer owned by a transport's separate map-object handle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use solarity_asset::{AnimationDataCatalog, DecodedM2Model};
use solarity_rendering::M2SequenceStartPhase;

use crate::application::game_object_behavior::GameObjectSceneSample;
use crate::application::model_playback::M2Playback;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

/// CPU playback survives GPU residency changes and completes once per scene update.
pub(in crate::application) struct TransportMapModel {
    animations: Arc<AnimationDataCatalog>,
    model: RefCell<Option<ModelState>>,
    requested_phase: Cell<Option<u32>>,
    scene_sample: RefCell<Option<GameObjectSceneSample>>,
}

/// One CPU resource lifetime and the timer borrowed by its GPU placement.
struct ModelState {
    display_id: u32,
    model: Arc<DecodedM2Model>,
    playback: Rc<RefCell<M2Playback>>,
}

impl TransportMapModel {
    pub(super) fn new(animations: Arc<AnimationDataCatalog>) -> Self {
        Self {
            animations,
            model: RefCell::new(None),
            requested_phase: Cell::new(None),
            scene_sample: RefCell::new(None),
        }
    }

    /// 7BDA70 binds an ordinary CM2Model; 7134A0 subsequently selects route phases.
    pub(super) fn attach(
        &self,
        display_id: u32,
        model: &Arc<DecodedM2Model>,
        phase: Option<u32>,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut current = self.model.borrow_mut();
        if current.as_ref().is_none_or(|current| {
            current.display_id != display_id || !Arc::ptr_eq(&current.model, model)
        }) {
            *current = Some(ModelState {
                display_id,
                model: Arc::clone(model),
                playback: Rc::new(RefCell::new(M2Playback::default_sequence(
                    model,
                    &self.animations,
                    scene_time_ms,
                    random,
                )?)),
            });
            self.requested_phase.set(None);
            self.scene_sample.borrow_mut().take();
        }
        if let Some(phase) = phase
            && self.requested_phase.get() != Some(phase)
            && let Some(current) = current.as_ref()
        {
            current.playback.borrow_mut().apply_model_sequence(
                model,
                &self.animations,
                phase,
                0,
                scene_time_ms,
                random,
            )?;
            // Behavior+38 is the route phase, independent of completion's new
            // CM2Model sequence. An unchanged phase must not restart ShipStart.
            self.requested_phase.set(Some(phase));
        }
        Ok(())
    }

    pub(super) fn detach(&self) {
        self.model.borrow_mut().take();
        self.scene_sample.borrow_mut().take();
        self.requested_phase.set(None);
    }

    pub(in crate::application) fn playback(&self) -> Option<Rc<RefCell<M2Playback>>> {
        self.model
            .borrow()
            .as_ref()
            .map(|model| Rc::clone(&model.playback))
    }

    /// 70B2B0 handles primary completions: Open/Close and ShipStart/ShipStop.
    pub(super) fn advance_scene(
        &self,
        scene_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.scene_sample.borrow_mut().take();
        let current = self.model.borrow();
        let Some(current) = current.as_ref() else {
            return Ok(());
        };
        let mut playback = current.playback.borrow_mut();
        let advance = playback.clock_with_completion(
            &current.model,
            scene_time_ms,
            global_time_ms,
            random,
            Some(&mut |playback, random| {
                let next = match playback.animation_id {
                    146 => 147,
                    148 => 149,
                    162 => 163,
                    164 => 0,
                    _ => return Ok(()),
                };
                let Some(resolved) = current
                    .model
                    .animations()
                    .resolve_model_animation(&self.animations, next)
                else {
                    return Ok(());
                };
                playback
                    .apply_resolved_model_sequence(
                        &current.model,
                        resolved.animation_id(),
                        resolved.mode(),
                        0,
                        playback.scene_time_ms,
                        M2SequenceStartPhase::DuringSceneUpdate,
                        false,
                        random,
                    )
                    .map(|_| ())
            }),
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
}
