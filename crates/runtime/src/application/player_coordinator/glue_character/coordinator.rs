//! Coalesces selected appearances on main; workers never wait for another decoder.

use super::super::worker_presentation::{
    AppearanceRequest, AppearanceTask, prepare_glue_character,
};
use super::super::{ResidentGlueCharacterKey, RuntimePlayerError, RuntimePlayerPresentation};
use super::PendingGlueCharacter;
use solarity_cpu::{CpuError, CpuExecutor};
use solarity_ui::{UiCharacterCreationPreview, UiCharacterSelectionPreview};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl RuntimePlayerPresentation {
    /// Submits character creation through shared primary-source readiness.
    pub(in crate::application) fn synchronize_character_creation_async(
        &mut self,
        preview: Option<&UiCharacterCreationPreview>,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimePlayerError> {
        self.synchronize_glue_character_async(
            preview.cloned().map(ResidentGlueCharacterKey::Creation),
            cpu,
        )
    }

    /// Submits roster selection through the same exact-generation appearance owner.
    pub(in crate::application) fn synchronize_character_selection_async(
        &mut self,
        preview: Option<&UiCharacterSelectionPreview>,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimePlayerError> {
        self.synchronize_glue_character_async(
            preview
                .cloned()
                .map(Box::new)
                .map(ResidentGlueCharacterKey::Selection),
            cpu,
        )
    }

    /// One admitted attempt owns the cache. Changed selections withdraw it before
    /// another can enter; transform-only changes consume the latest facing on main.
    fn synchronize_glue_character_async(
        &mut self,
        requested: Option<ResidentGlueCharacterKey>,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimePlayerError> {
        let request_changed = match (&self.requested_glue_character, &requested) {
            (Some(current), Some(requested)) => !current.same_residency(requested),
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        };
        self.requested_glue_character = requested.clone();
        if let Some(pending) = &mut self.pending_glue_character
            && requested
                .as_ref()
                .is_none_or(|key| !pending.matches(key, self.component_texture_level))
        {
            pending.withdraw();
        }
        if self
            .pending_glue_character
            .as_ref()
            .is_some_and(|pending| pending.task.is_finished())
        {
            let pending = self
                .pending_glue_character
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?;
            let current = requested
                .as_ref()
                .is_some_and(|key| pending.matches(key, self.component_texture_level));
            let completion = pending.task.join()?;
            self.glue_worker_cache = Some(completion.cache);
            match completion.result {
                Ok(Some(mut resident)) if current => {
                    let key = requested
                        .as_ref()
                        .unwrap_or_else(|| unreachable!("current completion has a selected key"));
                    resident.apply_transform_key(key.clone());
                    tracing::info!(
                        residency_wait_ms = pending.submitted_at.elapsed().as_secs_f64() * 1_000.0,
                        "published coalesced worker-prepared Glue character representation"
                    );
                    self.glue_character = Some(resident);
                    return Ok(true);
                }
                Ok(_) => {}
                Err(error) if current => {
                    self.failed_glue_character = requested;
                    return Err(error);
                }
                // Selection withdrawal intentionally cancels unstarted appearance work.
                Err(RuntimePlayerError::Cpu(CpuError::JobCancelled)) => {}
                Err(error) => {
                    tracing::warn!(error = %error, "retired obsolete Glue character preparation failure")
                }
            }
        }
        let Some(requested) = requested else {
            self.failed_glue_character = None;
            let removed = self.glue_character.take().is_some();
            return Ok(request_changed || removed);
        };
        if let Some(resident) = self.glue_character.as_mut()
            && resident.key.same_residency(&requested)
        {
            resident.apply_transform_key(requested);
            return Ok(request_changed);
        }
        if self
            .failed_glue_character
            .as_ref()
            .is_some_and(|failed| failed.same_residency(&requested))
        {
            return Ok(request_changed);
        }
        self.failed_glue_character = None;
        if self.pending_glue_character.is_some() {
            return Ok(request_changed);
        }
        let model_path = match self.glue_primary_path(&requested) {
            Ok(path) => path,
            Err(error) => {
                self.failed_glue_character = Some(requested);
                return Err(error);
            }
        };
        let catalog = self
            .glue_worker_catalog
            .as_ref()
            .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?
            .clone();
        let input = AppearanceRequest {
            catalog,
            catalogs: self.shared_catalogs(),
            level: self.component_texture_level,
            model_path,
        };
        let withdrawn = Arc::new(AtomicBool::new(false));
        let worker_withdrawn = Arc::clone(&withdrawn);
        let key = requested.clone();
        let Some(task) = AppearanceTask::submit(
            cpu,
            input,
            &mut self.glue_worker_cache,
            move |presentation, model| {
                if worker_withdrawn.load(Ordering::Acquire) {
                    return Ok(None);
                }
                let result = prepare_glue_character(presentation, key, model);
                // Dispose superseded derived output on its worker, as the old coalesced
                // step did. Main still rechecks the final completion/publication race.
                if worker_withdrawn.load(Ordering::Acquire) {
                    return Ok(None);
                }
                result.map(Some)
            },
        )?
        else {
            return Ok(request_changed);
        };
        self.pending_glue_character = Some(PendingGlueCharacter {
            submitted_at: std::time::Instant::now(),
            key: requested,
            level: self.component_texture_level,
            withdrawn,
            task,
        });
        Ok(request_changed)
    }
}
