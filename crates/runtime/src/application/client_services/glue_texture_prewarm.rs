//! Resumable source loading and covered GPU publication for configured Glue textures.

#[cfg(test)]
#[path = "../../../tests/application/glue_texture_prewarm.rs"]
mod tests;

use super::ClientServices;
use crate::application::{ApplicationError, archive_job::prepare_archive};
use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, BlpTextureCache};
use solarity_cpu::{CpuError, CpuTask};
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
    budget: solarity_asset::AssetReadBudget,
) -> impl FnMut() -> ControlFlow<Result<ConfiguredGlueTexturePrewarm, AssetError>> {
    let mut paths = paths.into_iter();
    let mut cache = BlpTextureCache::new();
    let mut failures = Vec::new();
    prepare_archive(catalog, move |store| {
        let _profile = solarity_profiling::profile!("runtime.glue_texture.source_step");
        let Some(path) = paths.next() else {
            return ControlFlow::Break(Ok(ConfiguredGlueTexturePrewarm {
                cache: std::mem::take(&mut cache),
                failures: std::mem::take(&mut failures),
            }));
        };
        if let Err(error) = store.with_read_budget(&budget, |store| cache.load(store, &path)) {
            failures.push(format!("failed to prewarm Glue texture {path}: {error}"));
        }
        ControlFlow::Continue(())
    })
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
                        let task = permit.submit_steps_with_context(
                            crate::application::archive_job::contextual(
                                "glue.texture.source_step",
                                prepare_configured_glue_textures(
                                    catalog,
                                    paths,
                                    solarity_asset::AssetReadBudget::for_service(
                                        self.cpu.storage().clone(),
                                        solarity_cpu::CpuService::Speculative,
                                    ),
                                ),
                            ),
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
        let uploaded =
            self.ui_texture_residency
                .prewarm(&mut self.renderer, &self.ui_textures, namespace)?;
        self.glue_gpu_texture_prewarm_pending = false;
        tracing::info!(
            uploaded_texture_count = uploaded,
            "published configured Glue textures behind transition cover"
        );
        Ok(())
    }
}
