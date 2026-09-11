//! Model-owned bone slots, shared callback scans and retained pose overrides.

use solarity_asset::{DecodedM2Model, M2ModelAnimationMode};
use solarity_rendering::{
    M2AnimationClock, M2CallbackSlot, M2EventTimeWindow, M2ModelSequenceBlend, M2QueuedCallback,
    M2SequenceStartPhase, scan_m2_callbacks,
};

use super::{M2ExpiredVariation, M2Playback, M2PlaybackAdvance};
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

#[derive(Clone)]
pub(super) struct M2BonePlayback {
    key: u16,
    bone: u16,
    order: u64,
    pub(super) playback: M2Playback,
}

/// The callback can change either slot before the next shared scan.
pub(in crate::application) type M2BoneCompletionCallback<'a> = dyn FnMut(&mut M2Playback, i32, u16, u32, &mut CrtRand) -> Result<(), RuntimeTerrainFrameError>
    + 'a;

/// Authored callbacks may synchronously change model timers and consume RNG.
pub(in crate::application) type M2BoneEventCallback<'a> = dyn FnMut(
        &mut M2Playback,
        usize,
        u32,
        &M2ExpiredVariation,
        &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError>
    + 'a;

impl M2Playback {
    pub(in crate::application) fn has_pending_sequence_callback(&self) -> bool {
        self.script_timer.is_some() && !self.script_finished
    }
    pub(in crate::application) fn bone_playback(&self, key: u16) -> Option<&Self> {
        self.bone_playback
            .iter()
            .find(|slot| slot.key == key)
            .map(|slot| &slot.playback)
    }

    fn bone_slot_mut(&mut self, bone: u16) -> Option<&mut Self> {
        if bone == 0 {
            Some(self)
        } else {
            self.bone_playback
                .iter_mut()
                .find(|slot| slot.bone == bone)
                .map(|slot| &mut slot.playback)
        }
    }

    fn ensure_bone_slot(&mut self, model: &DecodedM2Model, key: u16) -> Option<usize> {
        let bone = model
            .animations()
            .key_bone_lookup()
            .get(usize::from(key))
            .copied()
            .flatten()?;
        if bone == 0 {
            return None;
        }
        if let Some(index) = self.bone_playback.iter().position(|slot| slot.bone == bone) {
            return Some(index);
        }
        // Native bone timers start empty. A first primary selection does not
        // copy the inherited root into its previous-pose slot.
        let mut playback = Self::unstarted(0, self.created_scene_time_ms);
        playback.paused_scene_time_ms = self.paused_scene_time_ms;
        self.bone_playback.push(M2BonePlayback {
            key,
            bone,
            order: 0,
            playback,
        });
        Some(self.bone_playback.len() - 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn apply_bone_sequence(
        &mut self,
        model: &DecodedM2Model,
        key: u16,
        animation_id: u16,
        variation: Option<u16>,
        mode: M2ModelAnimationMode,
        speed: f32,
        offset: i32,
        now: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if model.animations().key_bone_lookup().get(usize::from(key)) == Some(&Some(0)) {
            return self.apply_resolved_model_sequence_variation(
                model,
                animation_id,
                variation,
                mode,
                speed,
                offset,
                now,
                phase,
                true,
                random,
            );
        }
        let Some(index) = self.ensure_bone_slot(model, key) else {
            return Ok(false);
        };
        let slot = &mut self.bone_playback[index];
        let newly_active = slot.playback.script_timer.is_none();
        let selected = slot.playback.apply_resolved_model_sequence_variation(
            model,
            animation_id,
            variation,
            mode,
            speed,
            offset,
            now,
            phase,
            true,
            random,
        )?;
        if selected && newly_active {
            self.next_activation_order += 1;
            self.bone_playback[index].order = self.next_activation_order;
            self.bone_playback
                .sort_by_key(|slot| std::cmp::Reverse(slot.order));
        }
        Ok(selected)
    }

    pub(in crate::application) fn set_bone_sequence_speed(
        &mut self,
        key: u16,
        speed: f32,
        now: u32,
    ) {
        if let Some(slot) = self.bone_playback.iter_mut().find(|slot| slot.key == key) {
            slot.playback.set_sequence_speed(speed, now);
        }
    }

    pub(in crate::application) fn set_bone_blend(
        &mut self,
        model: &DecodedM2Model,
        key: u16,
        blend: M2ModelSequenceBlend,
    ) {
        if let Some(index) = self.ensure_bone_slot(model, key) {
            self.bone_playback[index].playback.script_blend = Some(blend);
        }
    }

    pub(in crate::application) fn clear_bone_blend(&mut self, key: u16) {
        if let Some(slot) = self.bone_playback.iter_mut().find(|slot| slot.key == key) {
            slot.playback.script_blend = None;
        }
    }

    /// 832840 unlinks the primary immediately and keeps at most a 150 ms fade
    /// in the previous-pose slot. A stronger existing blend retains ownership.
    pub(in crate::application) fn clear_bone_sequence(
        &mut self,
        model: &DecodedM2Model,
        key: u16,
        blend: bool,
        now: u32,
    ) {
        let Some(slot) = self.bone_playback.iter_mut().find(|slot| slot.key == key) else {
            return;
        };
        if model.animations().bones()[usize::from(slot.bone)]
            .parent()
            .is_none()
        {
            return;
        }
        let playback = &mut slot.playback;
        if !blend {
            playback.script_blend = None;
        } else if playback
            .script_blend
            .is_none_or(|blend| blend.weight(now) <= 0.5)
        {
            playback.script_blend = playback
                .script_timer
                .map(|timer| M2ModelSequenceBlend::new(playback.sequence, timer, now, 150));
        }
        playback.script_timer = None;
        playback.sequence_duration_ms = 0.;
        playback.script_finished = true;
        slot.order = 0;
    }

    /// Samples all explicit bone timers without consuming events or RNG.
    pub(in crate::application) fn bone_sequence_clocks(
        &self,
        model: &DecodedM2Model,
        root_clock: M2AnimationClock,
        now: u32,
    ) -> Vec<(u16, M2AnimationClock)> {
        self.bone_playback
            .iter()
            .filter_map(|slot| {
                let playback = &slot.playback;
                if playback.script_timer.is_some() {
                    return Some((slot.key, playback.sample_clock(now)));
                }
                let blend = playback
                    .script_blend
                    .filter(|blend| blend.weight(now) > 0.)?;
                let mut base = root_clock;
                let mut parent = model.animations().bones()[usize::from(slot.bone)].parent();
                while let Some(bone) = parent {
                    if let Some(ancestor) = self.bone_playback.iter().find(|candidate| {
                        candidate.bone == bone && candidate.playback.script_timer.is_some()
                    }) {
                        base = ancestor.playback.sample_clock(now);
                        break;
                    }
                    parent = model.animations().bones()[usize::from(bone)].parent();
                }
                Some((
                    slot.key,
                    blend.apply_to_clock(base.without_secondary_sequence(), now),
                ))
            })
            .collect()
    }

    fn callback_slot(&self, bone: u16) -> Option<M2CallbackSlot> {
        Some(M2CallbackSlot {
            bone,
            sequence: self.sequence,
            timer: self.script_timer?,
            finished: self.script_finished,
        })
    }

    fn prepare_bone_scene(&mut self, now: u32) {
        self.scene_time_ms = now;
        if self.paused_scene_time_ms != 0 && now != 0 {
            let delta = now.wrapping_sub(self.paused_scene_time_ms);
            self.paused_scene_time_ms = now;
            if let Some(timer) = &mut self.script_timer {
                timer.shift_scene_time(delta);
            }
            if let Some(blend) = &mut self.script_blend {
                blend.shift_pose_time(delta);
            }
        }
    }

    /// Executes 831FC0's automatic tail after the user callback has returned.
    fn finish_bone_variation(
        &mut self,
        model: &DecodedM2Model,
        queued: M2QueuedCallback,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(timer) = self.script_timer else {
            return Ok(());
        };
        if self.sequence != queued.slot.sequence
            || timer.start_time_ms() != queued.slot.timer.start_time_ms()
            || timer.is_terminal()
            || !self.has_variations
        {
            return Ok(());
        }
        let animations = model.animations();
        let sequence = animations
            .select_model_sequence(self.animation_id, random.next_u15())
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: self.animation_id,
            })?;
        if animations.is_sequence_available(sequence) != Some(true) {
            return Ok(());
        }
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
        let timer = timer.restart_variation(
            &animations.sequences()[sequence],
            self.script_mode,
            self.scene_time_ms,
            queued.scene_time_ms,
            random.next_u15(),
        );
        self.sequence = sequence;
        self.sequence_duration_ms = animations.sequences()[sequence].duration_ms() as f32;
        self.cycle_count = timer.cycle_count();
        self.cycle_started_ms = timer.start_time_ms() as f32;
        self.has_variations = animations.sequences()[sequence].variation_index() != 0
            || animations.sequences()[sequence].variation_next().is_some();
        self.script_finished = timer.finished_on_activation(self.scene_time_ms);
        self.script_timer = Some(timer);
        Ok(())
    }

    pub(in crate::application) fn clock_with_bone_completion(
        &mut self,
        model: &DecodedM2Model,
        now: u32,
        random: &mut CrtRand,
        callback: Option<&mut M2BoneCompletionCallback<'_>>,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        self.clock_with_bone_callbacks(model, now, random, callback, None)
    }

    pub(in crate::application) fn clock_with_bone_callbacks(
        &mut self,
        model: &DecodedM2Model,
        now: u32,
        random: &mut CrtRand,
        mut callback: Option<&mut M2BoneCompletionCallback<'_>>,
        mut event_callback: Option<&mut M2BoneEventCallback<'_>>,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        self.prepare_bone_scene(now);
        for slot in &mut self.bone_playback {
            slot.playback.prepare_bone_scene(now);
        }
        let mut queue = std::mem::take(&mut self.callback_queue);
        let result = (|| {
            let mut expired_variations = Vec::new();
            let mut previous = self.previous_event_scene_time_ms;
            while self.paused_scene_time_ms == 0 {
                let slots = self
                    .bone_playback
                    .iter()
                    .filter(|slot| slot.order > self.activation_order)
                    .filter_map(|slot| slot.playback.callback_slot(slot.bone))
                    .chain(self.callback_slot(0))
                    .chain(
                        self.bone_playback
                            .iter()
                            .filter(|slot| slot.order <= self.activation_order)
                            .filter_map(|slot| slot.playback.callback_slot(slot.bone)),
                    );
                let nearest =
                    scan_m2_callbacks(model.animations(), slots, previous, now, true, &mut queue);
                if queue.is_empty() {
                    break;
                }
                // 830FB0 samples every queued event's position during the scan,
                // before any tied completion callback can replace either pose.
                let event_pose = queue.iter().any(|queued| queued.event.is_some()).then(|| {
                    let clock = self.sample_clock(now);
                    (clock, self.bone_sequence_clocks(model, clock, now))
                });
                for queued in queue.iter().copied() {
                    if let Some(index) = queued.event {
                        let Some((clock, bones)) = event_pose.as_ref() else {
                            continue;
                        };
                        let event = M2ExpiredVariation {
                            clock: *clock,
                            event_window: M2EventTimeWindow::queued_event(
                                queued.slot.sequence,
                                index,
                            ),
                            bone_sequences: bones.clone(),
                        };
                        if let Some(callback) = event_callback.as_mut() {
                            callback(self, index, queued.scene_time_ms, &event, random)?;
                        } else {
                            expired_variations.push(event);
                        }
                        continue;
                    }
                    let Some(slot) = self.bone_slot_mut(queued.slot.bone) else {
                        continue;
                    };
                    if slot.script_timer.is_none_or(|timer| {
                        timer.start_time_ms() != queued.slot.timer.start_time_ms()
                    }) || slot.sequence != queued.slot.sequence
                    {
                        continue;
                    }
                    slot.script_finished = queued.slot.timer.is_terminal();
                    let animation = slot.animation_id;
                    let bone = &model.animations().bones()[usize::from(queued.slot.bone)];
                    let key = if bone.parent().is_none() {
                        -1
                    } else {
                        bone.key_bone_id()
                    };
                    if let Some(callback) = callback.as_mut() {
                        callback(self, key, animation, queued.scene_time_ms, random)?;
                    }
                    if let Some(slot) = self.bone_slot_mut(queued.slot.bone) {
                        slot.finish_bone_variation(model, queued, random)?;
                    }
                }
                previous = nearest;
            }
            self.previous_event_scene_time_ms = now;
            if self
                .script_blend
                .is_some_and(|blend| blend.weight(now) == 0.)
            {
                self.script_blend = None;
            }
            for slot in &mut self.bone_playback {
                if slot
                    .playback
                    .script_blend
                    .is_some_and(|blend| blend.weight(now) == 0.)
                {
                    slot.playback.script_blend = None;
                }
            }
            Ok(M2PlaybackAdvance {
                clock: self.sample_clock(now),
                expired_variations,
            })
        })();
        queue.clear();
        self.callback_queue = queue;
        result
    }
}
