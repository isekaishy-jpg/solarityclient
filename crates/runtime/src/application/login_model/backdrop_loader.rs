//! Shared backdrop assets decoded through one worker-private archive owner.

#[cfg(test)]
#[path = "../../../tests/application/backdrop_loader.rs"]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureCache, DecodedM2Model, M2ModelCache,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};

use crate::application::terrain_frame::m2::GlueM2Texture;

use super::{RuntimeGlueModelError, load_glue_model_generation};

/// Immutable archive results shared by prewarm and selected-scene preparation.
pub(super) struct GlueBackdropAssets {
    pub(super) model: Arc<DecodedM2Model>,
    pub(super) textures: Vec<GlueM2Texture>,
}

/// One exact result is retained, including failure, without repeated archive requests.
type BackdropResult = Result<Arc<GlueBackdropAssets>, Arc<RuntimeGlueModelError>>;

/// Mounted handles and decode caches are only accessed by the active CPU job.
struct BackdropArchiveState {
    store: AssetStore,
    models: M2ModelCache,
    textures: BlpTextureCache,
}

/// Retains the catalog across capacity refusal and mounts it once on a worker.
struct BackdropArchiveOwner {
    catalog: ArchiveCatalog,
    state: Option<Result<BackdropArchiveState, Arc<RuntimeGlueModelError>>>,
}

impl BackdropArchiveOwner {
    /// Preserves exact precedence and remembers a failed mount instead of retrying it.
    fn load(&mut self, path: &AssetPath) -> BackdropResult {
        let state = self.state.get_or_insert_with(|| {
            AssetStore::mount(self.catalog.clone())
                .map(|store| BackdropArchiveState {
                    store,
                    models: M2ModelCache::new(),
                    textures: BlpTextureCache::new(),
                })
                .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))
        });
        let state = state.as_mut().map_err(|error| Arc::clone(error))?;
        let (model, textures) = load_glue_model_generation(
            &mut state.models,
            &mut state.textures,
            &mut state.store,
            path,
        )
        .map_err(Arc::new)?;
        Ok(Arc::new(GlueBackdropAssets { model, textures }))
    }
}

/// The scheduler never drops an active task when the requested scene changes.
struct PendingBackdrop {
    path: AssetPath,
    task: CpuTask<BackdropResult>,
}

/// Main-thread result cache with one finite, owned archive job at a time.
pub(super) struct GlueBackdropLoader {
    assets: Arc<Mutex<BackdropArchiveOwner>>,
    ready: HashMap<AssetPath, BackdropResult>,
    pending: Option<PendingBackdrop>,
}

impl GlueBackdropLoader {
    /// Captures discovery without opening another archive on the frame thread.
    pub(super) fn new(catalog: ArchiveCatalog) -> Self {
        Self {
            assets: Arc::new(Mutex::new(BackdropArchiveOwner {
                catalog,
                state: None,
            })),
            ready: HashMap::new(),
            pending: None,
        }
    }

    /// Returns a cached result or advances the selected path without blocking.
    ///
    /// A caller supplies prewarm paths only after its selected scene is ready.
    /// Changing the requested path therefore waits for at most the one active
    /// archive operation before the new request can take priority.
    pub(super) fn poll(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
    ) -> Result<Option<Arc<GlueBackdropAssets>>, Arc<RuntimeGlueModelError>> {
        self.collect_finished();
        if let Some(result) = self.ready.get(path) {
            return result.clone().map(Some);
        }
        if self.pending.is_none() {
            match self.submit(path, cpu) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.as_ref(),
                        RuntimeGlueModelError::Cpu(CpuError::AtCapacity { .. })
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// Explicit startup-only wait used to construct the hidden login scene before a movie.
    pub(super) fn load_blocking(&mut self, path: &AssetPath, cpu: &CpuExecutor) -> BackdropResult {
        self.collect_finished();
        loop {
            if let Some(result) = self.ready.get(path) {
                return result.clone();
            }
            if self.pending.is_none() {
                self.submit(path, cpu)?;
            }
            self.finish_active();
        }
    }

    /// Reports archive work still owned by this scheduler.
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Joins a finished operation without admitting unrelated speculative work.
    fn collect_finished(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.task.is_finished())
        {
            self.finish_active();
        }
    }

    /// Borrows shared worker state only inside the CPU job; refusal preserves its owner.
    fn submit(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
    ) -> Result<(), Arc<RuntimeGlueModelError>> {
        debug_assert!(self.pending.is_none());
        let assets = Arc::clone(&self.assets);
        let worker_path = path.clone();
        let task = cpu
            .try_submit(move || {
                let started = Instant::now();
                let result = assets
                    .lock()
                    .map_err(|_| Arc::new(RuntimeGlueModelError::BackdropWorkerUnavailable))?
                    .load(&worker_path);
                tracing::info!(model = %worker_path,
                elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0,
                "completed Glue backdrop archive job");
                result
            })
            .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))?;
        self.pending = Some(PendingBackdrop {
            path: path.clone(),
            task,
        });
        Ok(())
    }

    /// Stores the result under its original path, even after the selected scene changes.
    fn finish_active(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let result = pending
            .task
            .join()
            .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))
            .and_then(|result| result);
        self.ready.insert(pending.path, result);
    }
}

impl Drop for GlueBackdropLoader {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            // Only CPU-owned archive objects survive this join. No task is
            // abandoned when a client closes before its backdrop becomes ready.
            let _completed = pending.task.join();
        }
    }
}
