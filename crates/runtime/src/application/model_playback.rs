//! Retained model sequence ownership shared by gameplay and presentation.

#[cfg(test)]
#[path = "../../tests/application/model_playback.rs"]
mod tests;

use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;
use solarity_asset::{AnimationDataCatalog, DecodedM2Model, M2ModelAnimationMode};
use solarity_rendering::{
    M2AnimationClock, M2EventTimeWindow, M2ModelSequenceBlend, M2ModelSequenceTimer,
    M2SequenceStartPhase,
};
use solarity_systems::GameObjectAnimationRequest;

/// Per-instance sequence state retained by stock's `CM2Model` owner.
pub(in crate::application) struct M2Playback {
    pub(in crate::application) game_object_state: Option<u8>,
    pub(in crate::application) game_object_request: Option<u16>,
    pub(in crate::application) animation_id: u16,
    pub(in crate::application) sequence: usize,
    pub(in crate::application) sequence_duration_ms: f32,
    pub(in crate::application) cycle_count: u32,
    pub(in crate::application) cycle_started_ms: f32,
    pub(in crate::application) has_variations: bool,
    pub(in crate::application) previous_event_elapsed_ms: f32,
    pub(in crate::application) previous_global_event_elapsed_ms: f32,
    pub(in crate::application) event_timeline_started: bool,
    /// Explicit Model calls use native integer scene timers and event intervals.
    pub(in crate::application) script_timer: Option<M2ModelSequenceTimer>,
    pub(in crate::application) script_blend: Option<M2ModelSequenceBlend>,
    pub(in crate::application) script_mode: M2ModelAnimationMode,
    pub(in crate::application) scene_time_ms: u32,
    pub(in crate::application) previous_event_scene_time_ms: u32,
    script_finished: bool,
    paused_scene_time_ms: u32,
}

/// The native user callback runs before automatic variation selection.
pub(in crate::application) type M2CompletionCallback<'a> =
    dyn FnMut(&mut M2Playback, &mut CrtRand) -> Result<(), RuntimeTerrainFrameError> + 'a;

/// Current bone clock plus an expired variation tail awaiting event dispatch.
pub(in crate::application) struct M2PlaybackAdvance {
    pub(in crate::application) clock: M2AnimationClock,
    pub(in crate::application) expired_variations: Vec<M2ExpiredVariation>,
}

/// Final event interval and pose clock from one replaced sequence variation.
pub(in crate::application) struct M2ExpiredVariation {
    pub(in crate::application) clock: M2AnimationClock,
    pub(in crate::application) event_window: M2EventTimeWindow,
}

/// Cross-model animation identity copied by stock equipment components.
#[derive(Clone, Copy)]
pub(in crate::application) struct M2PlaybackSynchronization {
    animation_id: u16,
    variation_index: u16,
    cycle_count: u32,
    cycle_started_ms: f32,
    previous_global_event_elapsed_ms: f32,
}

impl M2Playback {
    /// Keeps static geometry and effects alive before a primary sequence exists.
    pub(in crate::application) fn unstarted(animation_id: u16) -> Self {
        Self {
            game_object_state: None,
            game_object_request: None,
            animation_id,
            sequence: 0,
            sequence_duration_ms: 0.0,
            cycle_count: 1,
            cycle_started_ms: 0.0,
            has_variations: false,
            previous_event_elapsed_ms: 0.0,
            previous_global_event_elapsed_ms: 0.0,
            event_timeline_started: false,
            script_timer: None,
            script_blend: None,
            script_mode: M2ModelAnimationMode::Forward,
            scene_time_ms: 0,
            previous_event_scene_time_ms: 0,
            script_finished: false,
            paused_scene_time_ms: 0,
        }
    }

    /// Starts a newly resident world owner against the existing scene clock.
    /// Native 0x00826B00 anchors sequence construction to that clock; loading a
    /// neighbor must not age its new local sequence from world entry time zero.
    pub(in crate::application) fn new_at(
        model: &DecodedM2Model,
        animation_id: u16,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<Option<Self>, RuntimeTerrainFrameError> {
        let mut playback = Self::new(model, animation_id, random)?;
        if let Some(playback) = playback.as_mut() {
            playback.cycle_started_ms = scene_time_ms;
            playback.scene_time_ms = scene_time_ms as u32;
            playback.previous_event_scene_time_ms = scene_time_ms as u32;
        }
        Ok(playback)
    }

    /// Selects one base animation and consumes its authored cycle-count roll.
    pub(in crate::application) fn new(
        model: &DecodedM2Model,
        animation_id: u16,
        random: &mut CrtRand,
    ) -> Result<Option<Self>, RuntimeTerrainFrameError> {
        let animations = model.animations();
        if animations.sequences().is_empty() {
            return Ok(Some(Self::unstarted(animation_id)));
        }
        let sequence = animations
            .sequence_for_variation(animation_id, 0)
            .or_else(|| {
                animations.select_sequence(animation_id, None, u32::from(random.next_u15()))
            });
        let Some(sequence) = sequence else {
            tracing::debug!(
                path = %model.path(),
                animation_id,
                "placed M2 omitted because its selected animation is unavailable"
            );
            return Ok(None);
        };
        let sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        let cycle_count = animations.sequences()[sequence].cycle_count(random.next_u15());
        let variation_count = animations
            .available_variation_count(animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        Ok(Some(Self {
            game_object_state: None,
            game_object_request: None,
            animation_id,
            sequence,
            sequence_duration_ms,
            cycle_count,
            cycle_started_ms: 0.0,
            has_variations: variation_count > 1,
            previous_event_elapsed_ms: 0.0,
            previous_global_event_elapsed_ms: 0.0,
            event_timeline_started: false,
            script_timer: None,
            script_blend: None,
            script_mode: M2ModelAnimationMode::Forward,
            scene_time_ms: 0,
            previous_event_scene_time_ms: 0,
            script_finished: false,
            paused_scene_time_ms: 0,
        }))
    }

    /// Applies each Model Lua request without replacing the mutable M2 owner.
    pub(in crate::application) fn apply_model_sequence(
        &mut self,
        model: &DecodedM2Model,
        catalog: &AnimationDataCatalog,
        requested_animation: u32,
        time_offset_ms: i32,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        // 0x00832840 refuses to clear bone zero. The -1 animation sentinel
        // therefore leaves a Model widget's primary timer and RNG untouched.
        let animations = model.animations();
        if requested_animation == u32::MAX || animations.bones().is_empty() {
            return Ok(());
        }
        let Some(resolved) = animations.resolve_model_animation(catalog, requested_animation)
        else {
            return Ok(());
        };
        let animation_id = resolved.animation_id();
        self.apply_resolved_model_sequence(
            model,
            animation_id,
            resolved.mode(),
            time_offset_ms,
            scene_time_ms,
            M2SequenceStartPhase::BeforeSceneUpdate,
            false,
            random,
        )
        .map(|_| ())
    }

    /// Changes a stable generic GameObject request without resetting live neighbors.
    pub(in crate::application) fn select_game_object_state(
        &mut self,
        model: &DecodedM2Model,
        catalog: &AnimationDataCatalog,
        state: u8,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.game_object_state == Some(state) {
            return Ok(());
        }
        let animations = model.animations();
        // 0x00832AB0 also validates the primary bone before selecting or rolling.
        if !animations.bones().is_empty() {
            let request =
                GameObjectAnimationRequest::resolve(animations, game_object_animation_id(state));
            if self
                .game_object_request
                .is_some_and(|current| request.preserves_current(current))
            {
                self.game_object_state = Some(state);
                return Ok(());
            }
            if let Some(resolved) =
                animations.resolve_model_animation(catalog, u32::from(request.animation_id()))
            {
                // Frozen substitutions always name an authored Open/Close clip,
                // so CM2Model cannot add a reverse/endpoint fallback operation.
                let mode = if request.frozen() {
                    M2ModelAnimationMode::HoldStart
                } else {
                    resolved.mode()
                };
                self.apply_resolved_model_sequence(
                    model,
                    resolved.animation_id(),
                    mode,
                    0,
                    scene_time_ms,
                    M2SequenceStartPhase::BeforeSceneUpdate,
                    false,
                    random,
                )?;
                self.game_object_request = Some(request.animation_id());
            }
        }
        self.game_object_state = Some(state);
        Ok(())
    }

    /// Shared 0x00832AB0 variation selection and 0x00826B00 timer construction.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn apply_resolved_model_sequence(
        &mut self,
        model: &DecodedM2Model,
        animation_id: u16,
        mode: M2ModelAnimationMode,
        time_offset_ms: i32,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        blend: bool,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        self.apply_resolved_model_sequence_variation(
            model,
            animation_id,
            None,
            mode,
            time_offset_ms,
            scene_time_ms,
            phase,
            blend,
            random,
        )
    }

    /// Unit death callbacks explicitly retain the outgoing variation ordinal.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn apply_resolved_model_sequence_variation(
        &mut self,
        model: &DecodedM2Model,
        animation_id: u16,
        variation: Option<u16>,
        mode: M2ModelAnimationMode,
        time_offset_ms: i32,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        blend: bool,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let animations = model.animations();
        let explicit_sequence = variation
            .and_then(|variation| animations.model_sequence_for_variation(animation_id, variation));
        let automatic_variations = explicit_sequence.is_none();
        let sequence = explicit_sequence
            .or_else(|| animations.select_model_sequence(animation_id, random.next_u15()))
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        if animations.is_sequence_available(sequence) != Some(true) {
            // Native selection queues unavailable external animation data
            // after consuming its variation roll. The previous timer survives.
            return Ok(false);
        }
        let timer = M2ModelSequenceTimer::new(
            &animations.sequences()[sequence],
            mode,
            // 0x00826B00 reads the owning scene clock at the request, even
            // when this model has not been sampled while its widget is hidden.
            scene_time_ms,
            time_offset_ms,
            random.next_u15(),
            phase,
        );
        if blend {
            if let Some(previous) = self.script_timer
                && self
                    .script_blend
                    .is_none_or(|blend| blend.weight(scene_time_ms) <= 0.5)
            {
                self.script_blend = Some(M2ModelSequenceBlend::new(
                    self.sequence,
                    previous,
                    scene_time_ms,
                    animations.sequences()[sequence].blend_time_ms(),
                ));
            }
        } else {
            self.script_blend = None;
        }
        self.animation_id = animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = animations.sequences()[sequence].duration_ms() as f32;
        self.cycle_count = timer.cycle_count();
        self.cycle_started_ms = timer.start_time_ms() as f32;
        self.has_variations = automatic_variations
            && (animations.sequences()[sequence].variation_index() != 0
                || animations.sequences()[sequence].variation_next().is_some());
        self.script_timer = Some(timer);
        self.script_finished = false;
        self.script_mode = mode;
        Ok(true)
    }

    /// Applies the native model pause marker without resetting a sequence.
    pub(in crate::application) fn set_paused(&mut self, paused: bool, scene_time_ms: u32) {
        self.paused_scene_time_ms = if paused { scene_time_ms.max(1) } else { 0 };
    }

    /// Restarts playback when authoritative gameplay selects another base ID.
    pub(in crate::application) fn select_animation(
        &mut self,
        model: &DecodedM2Model,
        animation_id: u16,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.animation_id == animation_id || model.animations().sequences().is_empty() {
            self.animation_id = animation_id;
            return Ok(());
        }
        let animations = model.animations();
        let sequence = animations
            .sequence_for_variation(animation_id, 0)
            .or_else(|| {
                animations.select_sequence(animation_id, None, u32::from(random.next_u15()))
            })
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        self.animation_id = animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        self.cycle_count = animations.sequences()[sequence].cycle_count(random.next_u15());
        self.cycle_started_ms = animation_time_ms;
        self.previous_event_elapsed_ms = 0.0;
        self.event_timeline_started = false;
        self.has_variations = animations
            .available_variation_count(animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?
            > 1;
        Ok(())
    }

    /// Advances one expired stock timer and returns the selected sequence clock.
    pub(in crate::application) fn clock(
        &mut self,
        model: &DecodedM2Model,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        self.clock_with_completion(model, animation_time_ms, global_time_ms, random, None)
    }

    /// Advances a model whose gameplay owner registered a primary sequence callback.
    pub(in crate::application) fn clock_with_completion(
        &mut self,
        model: &DecodedM2Model,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
        callback: Option<&mut M2CompletionCallback<'_>>,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        self.scene_time_ms = animation_time_ms as u32;
        if let Some(timer) = self.script_timer {
            return self.advance_model_timer(model, timer, global_time_ms, random, callback);
        }
        let elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let selected_span_ms = self.sequence_duration_ms * self.cycle_count as f32;
        let mut expired_variations = Vec::new();
        if self.has_variations && self.sequence_duration_ms > 0.0 && elapsed_ms >= selected_span_ms
        {
            // Stock finishes the old sequence's event interval before replacing
            // its timer. Bone-relative callbacks from that tail must also use
            // the old sequence's terminal pose, not the newly selected pose.
            expired_variations.push(M2ExpiredVariation {
                clock: M2AnimationClock::new(
                    self.sequence,
                    self.sequence_duration_ms,
                    global_time_ms,
                ),
                event_window: M2EventTimeWindow::new(
                    self.sequence,
                    self.previous_event_elapsed_ms,
                    selected_span_ms,
                    !self.event_timeline_started,
                    true,
                ),
            });
            let animation_id = model.animations().sequences()[self.sequence].animation_id();
            self.sequence = model
                .animations()
                .select_sequence(animation_id, None, u32::from(random.next_u15()))
                .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                    model: model.path().clone(),
                    animation_id,
                })?;
            self.sequence_duration_ms = resolved_sequence_duration(model, self.sequence)?;
            self.cycle_count =
                model.animations().sequences()[self.sequence].cycle_count(random.next_u15());
            // This legacy gameplay path still restarts at the current frame.
            // Explicit Model calls below retain native callback overdue time.
            self.cycle_started_ms = animation_time_ms;
            self.previous_event_elapsed_ms = 0.0;
            self.event_timeline_started = false;
        }
        Ok(M2PlaybackAdvance {
            clock: world_animation_clock(
                self.sequence,
                self.sequence_duration_ms,
                animation_time_ms - self.cycle_started_ms,
                global_time_ms,
            ),
            expired_variations,
        })
    }

    /// Dispatches native cycle boundaries before sampling the remaining frame interval.
    fn advance_model_timer(
        &mut self,
        model: &DecodedM2Model,
        mut timer: M2ModelSequenceTimer,
        global_time_ms: f32,
        random: &mut CrtRand,
        mut callback: Option<&mut M2CompletionCallback<'_>>,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        let animations = model.animations();
        let mut expired_variations = Vec::new();
        if self.paused_scene_time_ms != 0 && self.scene_time_ms != 0 {
            let delta = self.scene_time_ms.wrapping_sub(self.paused_scene_time_ms);
            self.paused_scene_time_ms = self.scene_time_ms;
            timer.shift_scene_time(delta);
            if let Some(blend) = self.script_blend.as_mut() {
                blend.shift_pose_time(delta);
            }
        }
        while self.paused_scene_time_ms == 0
            && !self.script_finished
            && (self.has_variations || callback.is_some())
        {
            let Some(boundary) =
                timer.next_completion_ms(self.previous_event_scene_time_ms, self.scene_time_ms)
            else {
                break;
            };
            expired_variations.push(M2ExpiredVariation {
                clock: M2AnimationClock::new(
                    self.sequence,
                    timer.animation_time_ms(boundary) as f32,
                    global_time_ms,
                ),
                event_window: M2EventTimeWindow::new(self.sequence, 0.0, 0.0, false, false)
                    .with_scene_timer(timer, self.previous_event_scene_time_ms, boundary),
            });
            self.previous_event_scene_time_ms = boundary;
            self.script_timer = Some(timer);
            self.script_finished = timer.is_terminal();
            if let Some(callback) = callback.as_mut() {
                let previous_sequence = self.sequence;
                callback(self, random)?;
                if let Some(next_timer) = self.script_timer
                    && (self.sequence != previous_sequence
                        || next_timer.start_time_ms() != timer.start_time_ms())
                {
                    timer = next_timer;
                    continue;
                }
            }
            if timer.is_terminal() {
                break;
            }
            if !self.has_variations {
                continue;
            }
            let sequence = animations
                .select_model_sequence(self.animation_id, random.next_u15())
                .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                    model: model.path().clone(),
                    animation_id: self.animation_id,
                })?;
            if animations.is_sequence_available(sequence) != Some(true) {
                break;
            }
            // 0x00826C40 keeps an existing secondary while its contribution
            // is strictly above one half. Otherwise the outgoing primary
            // replaces it, using the incoming sequence's blend duration.
            if self
                .script_blend
                .is_none_or(|blend| blend.weight(self.scene_time_ms) <= 0.5)
            {
                self.script_blend = Some(M2ModelSequenceBlend::new(
                    self.sequence,
                    timer,
                    self.scene_time_ms,
                    animations.sequences()[sequence].blend_time_ms(),
                ));
            }
            timer = timer.restart_variation(
                &animations.sequences()[sequence],
                self.script_mode,
                self.scene_time_ms,
                boundary,
                random.next_u15(),
            );
            self.sequence = sequence;
            self.sequence_duration_ms = animations.sequences()[sequence].duration_ms() as f32;
            self.cycle_count = timer.cycle_count();
            self.cycle_started_ms = timer.start_time_ms() as f32;
            self.has_variations = animations.sequences()[sequence].variation_index() != 0
                || animations.sequences()[sequence].variation_next().is_some();
            self.script_finished = false;
        }
        self.script_timer = Some(timer);
        let mut clock = M2AnimationClock::new(
            self.sequence,
            timer.animation_time_ms(self.scene_time_ms) as f32,
            global_time_ms,
        );
        if let Some(blend) = self.script_blend {
            if blend.weight(self.scene_time_ms) == 0.0 {
                self.script_blend = None;
            } else {
                clock = blend.apply_to_clock(clock, self.scene_time_ms);
            }
        }
        Ok(M2PlaybackAdvance {
            clock,
            expired_variations,
        })
    }

    /// Advances the unwrapped clocks retained exclusively for event crossing.
    pub(in crate::application) fn event_window(
        &mut self,
        animation_time_ms: f32,
        global_time_ms: f32,
    ) -> M2EventTimeWindow {
        if let Some(timer) = self.script_timer {
            let window = M2EventTimeWindow::new(self.sequence, 0.0, 0.0, false, false)
                .with_scene_timer(
                    timer,
                    if self.paused_scene_time_ms != 0 {
                        animation_time_ms as u32
                    } else {
                        self.previous_event_scene_time_ms
                    },
                    animation_time_ms as u32,
                );
            self.previous_event_scene_time_ms = animation_time_ms as u32;
            return window;
        }
        self.previous_event_scene_time_ms = animation_time_ms as u32;
        let current_event_elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let window = M2EventTimeWindow::new(
            self.sequence,
            self.previous_event_elapsed_ms,
            current_event_elapsed_ms,
            !self.event_timeline_started,
            true,
        )
        .with_global_time(self.previous_global_event_elapsed_ms, global_time_ms);
        self.previous_event_elapsed_ms = current_event_elapsed_ms;
        self.previous_global_event_elapsed_ms = global_time_ms;
        self.event_timeline_started = true;
        window
    }

    /// Captures the stock sequence identity shared with an equipment model.
    pub(in crate::application) fn synchronization(
        &self,
        model: &DecodedM2Model,
    ) -> M2PlaybackSynchronization {
        let variation_index = model
            .animations()
            .sequences()
            .get(self.sequence)
            .map_or(0, |sequence| sequence.variation_index());
        M2PlaybackSynchronization {
            animation_id: self.animation_id,
            variation_index,
            cycle_count: self.cycle_count,
            cycle_started_ms: self.cycle_started_ms,
            previous_global_event_elapsed_ms: self.previous_global_event_elapsed_ms,
        }
    }

    /// Maps another model's active sequence identity onto this model's table.
    pub(in crate::application) fn synchronize_from(
        &mut self,
        model: &DecodedM2Model,
        source: M2PlaybackSynchronization,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let sequence = model
            .animations()
            .select_sequence(source.animation_id, Some(source.variation_index), 0)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: source.animation_id,
            })?;
        let identity_changed = self.animation_id != source.animation_id
            || self.sequence != sequence
            || self.cycle_started_ms != source.cycle_started_ms;
        self.animation_id = source.animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        self.cycle_count = source.cycle_count;
        self.cycle_started_ms = source.cycle_started_ms;
        self.has_variations = model
            .animations()
            .available_variation_count(source.animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: source.animation_id,
            })?
            > 1;
        if identity_changed {
            self.previous_event_elapsed_ms = 0.0;
            self.previous_global_event_elapsed_ms = source.previous_global_event_elapsed_ms;
            self.event_timeline_started = false;
        }
        Ok(())
    }
}

/// Resolves the immutable duration owned by an alias target.
fn resolved_sequence_duration(
    model: &DecodedM2Model,
    sequence: usize,
) -> Result<f32, RuntimeTerrainFrameError> {
    let resolved = model
        .animations()
        .resolve_sequence_alias(sequence)
        .ok_or_else(|| RuntimeTerrainFrameError::M2SequenceIndex {
            model: model.path().clone(),
            sequence,
        })?;
    Ok(model.animations().sequences()[resolved].duration_ms() as f32)
}

/// Advances the sequence selected for one placed model instance.
fn world_animation_clock(
    sequence: usize,
    duration_ms: f32,
    animation_time_ms: f32,
    global_time_ms: f32,
) -> M2AnimationClock {
    let animation_time_ms = if duration_ms > 0.0 {
        animation_time_ms.rem_euclid(duration_ms)
    } else {
        0.0
    };
    M2AnimationClock::new(sequence, animation_time_ms, global_time_ms)
}

/// Stable generic behavior request; progress, transition clips, and completion
/// callbacks additionally require the retained GameObject behavior clock.
const fn game_object_animation_id(state: u8) -> u16 {
    match state {
        1 => 147,
        2 => 151,
        _ => 149,
    }
}
