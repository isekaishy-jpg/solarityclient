//! Resumable source loading and covered GPU publication for configured Glue textures.

#[cfg(test)]
#[path = "../../../tests/application/glue_texture_prewarm.rs"]
mod tests;

use super::ClientServices;
use crate::application::texture_source_job::SharedTextureSources;
use crate::application::{ApplicationError, archive_job::prepare_archive_resumable};
use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, BlpTextureCache};
use solarity_cpu::{CpuError, CpuTask, CpuTaskStep, JobContext};
use std::ops::ControlFlow;

/// Completed immutable sources and the existing per-path speculative diagnostics.
pub(super) struct ConfiguredGlueTexturePrewarm {
    cache: BlpTextureCache,
    failures: Vec<String>,
}

/// Admission refusal keeps the original catalog and ordered demand untouched.
pub(super) enum ConfiguredGlueTexturePrewarmJob {
    Deferred {
        catalog: ArchiveCatalog,
        paths: Vec<AssetPath>,
    },
    Running(CpuTask<Result<ConfiguredGlueTexturePrewarm, AssetError>>),
}

/// Opens one archive per turn, then reads/decodes at most one texture per turn.
/// The authored source format and existing speculative failure policy are unchanged.
pub(super) fn prepare_configured_glue_textures(
    catalog: ArchiveCatalog,
    paths: Vec<AssetPath>,
    shared: SharedTextureSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<ConfiguredGlueTexturePrewarm, AssetError>> {
    let mut paths = paths.into_iter();
    let mut cache = BlpTextureCache::new();
    let mut failures = Vec::new();
    let mut pending = None;
    let mut operation = prepare_archive_resumable(catalog, move |store| {
        let Some(path) = paths.as_slice().first().cloned() else {
            return CpuTaskStep::Complete(Ok(ConfiguredGlueTexturePrewarm {
                cache: std::mem::take(&mut cache),
                failures: std::mem::take(&mut failures),
            }));
        };
        let result = match shared.load(store, &path, &mut pending) {
            Ok(ControlFlow::Continue(edge)) => return CpuTaskStep::Wait(edge),
            Ok(ControlFlow::Break(source)) => store
                .with_read_budget(&shared.read_budget(), |store| cache.adopt(store, source))
                .map(|_| ())
                .map_err(solarity_asset::BlpLoadError::from),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            failures.push(format!("failed to prewarm Glue texture {path}: {error}"));
        }
        paths.next();
        CpuTaskStep::Continue
    });
    move |context| {
        context.diagnostic_value("glue.texture.source_step", 1);
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(CpuError::JobCancelled.into()));
        }
        operation()
    }
}

impl ClientServices {
    /// Adopts a completed speculative Glue source cache without waiting.
    pub(super) fn poll_glue_texture_prewarm(&mut self) {
        let Some(pending) = self.pending_glue_texture_prewarm.take() else {
            return;
        };
        let pending = match pending {
            ConfiguredGlueTexturePrewarmJob::Deferred { catalog, paths } => {
                match self.cpu.can_admit_speculative() {
                    Ok(false) => {
                        self.pending_glue_texture_prewarm =
                            Some(ConfiguredGlueTexturePrewarmJob::Deferred { catalog, paths });
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "could not inspect Glue texture prewarm capacity");
                        self.developer_console.record_error(&message);
                        return;
                    }
                    Ok(true) => {}
                }
                match self
                    .cpu
                    .try_reserve_for(solarity_cpu::CpuService::Speculative)
                {
                    Ok(permit) => {
                        let shared = SharedTextureSources {
                            budget: self.cpu.storage().clone(),
                            service: permit.service_control(),
                        };
                        let task = permit.submit_resumable_with_context(
                            prepare_configured_glue_textures(catalog, paths, shared),
                        );
                        self.pending_glue_texture_prewarm =
                            Some(ConfiguredGlueTexturePrewarmJob::Running(task));
                        return;
                    }
                    Err(CpuError::AtCapacity { .. }) => {
                        self.pending_glue_texture_prewarm =
                            Some(ConfiguredGlueTexturePrewarmJob::Deferred { catalog, paths });
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "could not submit deferred Glue texture prewarm");
                        self.developer_console.record_error(&message);
                        return;
                    }
                }
            }
            ConfiguredGlueTexturePrewarmJob::Running(task) if !task.is_finished() => {
                self.pending_glue_texture_prewarm =
                    Some(ConfiguredGlueTexturePrewarmJob::Running(task));
                return;
            }
            ConfiguredGlueTexturePrewarmJob::Running(task) => task,
        };
        match pending.join() {
            Ok(Ok(prepared)) => {
                let admitted = self.ui_textures.merge(prepared.cache);
                self.glue_gpu_texture_prewarm_pending |= admitted != 0;
                tracing::info!(
                    admitted_texture_count = admitted,
                    resident_texture_count = self.ui_textures.len(),
                    failed_texture_count = prepared.failures.len(),
                    "adopted configured Glue texture prewarm"
                );
                for failure in prepared.failures {
                    tracing::warn!(error = %failure, "configured Glue texture source failed to prewarm");
                    self.developer_console.record_error(&failure);
                }
            }
            Ok(Err(error)) => {
                let message = error.to_string();
                tracing::warn!(error = %message, "configured Glue texture prewarm failed");
                self.developer_console.record_error(&message);
            }
            Err(error) => {
                let message = error.to_string();
                tracing::warn!(error = %message, "configured Glue texture worker failed");
                self.developer_console.record_error(&message);
            }
        }
    }

    /// Publishes worker-decoded Glue images only while an authored cover is visible.
    pub(super) fn service_glue_gpu_texture_prewarm(&mut self) -> Result<(), ApplicationError> {
        let covered =
            self.authentication_prewarm_active || self.glue.media_intent().movie().is_some();
        if !covered || !self.glue_gpu_texture_prewarm_pending {
            return Ok(());
        }
        let namespace = self.assets.borrow().namespace();
        let uploaded = self.ui_texture_residency.prewarm(
            &mut solarity_rendering::GpuPreparation::new(
                &mut self.renderer,
                &mut crate::application::frame_pipeline::FrameWait::Native(&mut self.platform)
                    .recording(&self.cpu),
            ),
            &self.ui_textures,
            namespace,
        )?;
        self.glue_gpu_texture_prewarm_pending = false;
        tracing::info!(
            uploaded_texture_count = uploaded,
            "published configured Glue textures behind transition cover"
        );
        Ok(())
    }
}
