//! Per-widget sequence ownership across asynchronous model loading and GPU activation.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use solarity_asset::{AssetPath, DecodedM2Model};
use solarity_cpu::CpuExecutor;
use solarity_ui::{GlueManager, UiModelAction, UiModelInstance};

use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::terrain_frame::m2::M2Playback;
use crate::random::CrtRand;

use super::{GlueModelKey, RuntimeGlueModelError, RuntimeGlueModelScene};

/// Mutable sequence state exists independently of the visible GPU compositor.
pub(super) struct GlueScriptModelInstance {
    identity: UiModelInstance,
    path: AssetPath,
    model: Option<Arc<DecodedM2Model>>,
    /// `None` after activation means that the active GPU frame owns playback.
    playback: Option<M2Playback>,
    started_at: Instant,
    /// Animation ID and signed time offset in exact per-instance call order.
    requests: VecDeque<(u32, i32)>,
}

impl GlueScriptModelInstance {
    /// Associates a published presentation with its exact mutable native owner.
    fn matches(&self, key: &GlueModelKey) -> bool {
        self.identity.object_index() == key.object_index
            && self.identity.generation() == key.instance_generation
            && self.path == key.path
    }
}

impl RuntimeGlueModelScene {
    /// Drains ordered Lua calls and services both visible and hidden model loads.
    ///
    /// Calls against resident instances execute before the next Lua action is
    /// consumed. Requests made during loading remain FIFO until its immutable
    /// source arrives; replacing that instance cancels its pending requests.
    pub(crate) fn synchronize_script_models(
        &mut self,
        glue: &GlueManager,
        cpu: &CpuExecutor,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeGlueModelError> {
        while let Some(action) = glue.take_model_action() {
            match action {
                UiModelAction::Assign { instance, path } => {
                    self.model_instances
                        .retain(|entry| entry.identity.object_index() != instance.object_index());
                    if let Some(path) = path {
                        self.model_instances.push(GlueScriptModelInstance {
                            identity: instance,
                            path,
                            model: None,
                            playback: None,
                            started_at: Instant::now(),
                            requests: VecDeque::new(),
                        });
                        self.service_script_instance(self.model_instances.len() - 1, cpu, random)?;
                    }
                }
                UiModelAction::Sequence {
                    instance,
                    animation_id,
                    time_offset_ms,
                } => {
                    if let Some(index) = self
                        .model_instances
                        .iter()
                        .position(|entry| entry.identity == instance)
                    {
                        self.model_instances[index]
                            .requests
                            .push_back((animation_id, time_offset_ms));
                        self.service_script_instance(index, cpu, random)?;
                    }
                }
            }
        }
        // Vec order preserves instance creation order for simultaneous load
        // completions. A randomized HashMap order would change the CRT stream.
        for index in 0..self.model_instances.len() {
            self.service_script_instance(index, cpu, random)?;
        }
        Ok(())
    }

    /// Initializes lightweight playback when archive decoding completes, without GPU work.
    fn service_script_instance(
        &mut self,
        index: usize,
        cpu: &CpuExecutor,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeGlueModelError> {
        let entry = &mut self.model_instances[index];
        if entry.model.is_none() {
            let Some(loaded) = self
                .backdrop_assets
                .poll(&entry.path, cpu)
                .map_err(|source| RuntimeGlueModelError::BackdropLoad {
                    path: entry.path.clone(),
                    source,
                })?
            else {
                return Ok(());
            };
            let animation_id = loaded
                .model
                .animations()
                .resolve_model_animation(&self.animations, 0)
                .map_or(0, |resolved| resolved.animation_id());
            entry.playback = Some(
                M2Playback::new(&loaded.model, animation_id, random)?.ok_or_else(|| {
                    RuntimeTerrainFrameError::M2AnimationSelection {
                        model: entry.path.clone(),
                        animation_id,
                    }
                })?,
            );
            entry.model = Some(Arc::clone(&loaded.model));
            entry.started_at = Instant::now();
        }
        while let Some((animation_id, time_offset_ms)) = entry.requests.pop_front() {
            if let Some(active) = self
                .active
                .as_mut()
                .filter(|active| entry.matches(&active.key))
            {
                active.frame.apply_glue_sequence(
                    &self.animations,
                    animation_id,
                    time_offset_ms,
                    random,
                )?;
            } else {
                let playback = entry
                    .playback
                    .as_mut()
                    .ok_or(RuntimeGlueModelError::PendingState)?;
                let model = entry
                    .model
                    .as_ref()
                    .ok_or(RuntimeGlueModelError::PendingState)?;
                playback.apply_model_sequence(
                    model,
                    &self.animations,
                    animation_id,
                    time_offset_ms,
                    (entry.started_at.elapsed().as_secs_f32() * 1_000.0) as u32,
                    random,
                )?;
            }
            tracing::debug!(
                object_index = entry.identity.object_index(),
                instance_generation = entry.identity.generation(),
                animation_id,
                time_offset_ms,
                "applied ordered Glue model sequence request"
            );
        }
        Ok(())
    }

    /// Returns the retiring frame's mutable timer to its still-live widget.
    pub(super) fn retire_active_script_model(&mut self) {
        if let Some(mut active) = self.active.take()
            && let Some(entry) = self
                .model_instances
                .iter_mut()
                .find(|entry| entry.matches(&active.key))
        {
            entry.playback = active.frame.take_glue_playback();
        }
    }

    /// Transfers an already-initialized timer into one visible GPU frame.
    pub(super) fn take_script_playback(
        &mut self,
        key: &GlueModelKey,
    ) -> Result<(M2Playback, Instant), RuntimeGlueModelError> {
        let entry = self
            .model_instances
            .iter_mut()
            .find(|entry| entry.matches(key))
            .ok_or(RuntimeGlueModelError::PendingState)?;
        Ok((
            entry
                .playback
                .take()
                .ok_or(RuntimeGlueModelError::PendingState)?,
            entry.started_at,
        ))
    }
}
