//! Per-loader publication preserves request order and independent scene lifetime.

use super::{
    BackdropArchiveOwner, BackdropResult, GlueBackdropAssets, GlueBackdropLoader,
    RuntimeGlueModelError,
};
use solarity_asset::{ArchiveCatalog, AssetPath};
use solarity_cpu::{CpuError, CpuExecutor, CpuService};
use std::{collections::HashMap, sync::Arc};

impl GlueBackdropLoader {
    /// Captures discovery without opening another archive on the frame thread.
    pub(in crate::application::login_model) fn new(catalog: ArchiveCatalog) -> Self {
        Self {
            sources: catalog.model_cache_service(),
            namespace: catalog.namespace(),
            waiting: None,
            assets: Some(BackdropArchiveOwner {
                catalog,
                state: None,
            }),
            ready: HashMap::new(),
            pending: None,
        }
    }

    /// Returns a cached result or advances the selected path without blocking.
    ///
    /// A caller supplies prewarm paths only after its selected scene is ready.
    /// Changing the requested path therefore waits for at most the one active
    /// archive operation before the new request can take priority.
    pub(in crate::application::login_model) fn poll(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
    ) -> Result<Option<Arc<GlueBackdropAssets>>, Arc<RuntimeGlueModelError>> {
        self.poll_for(path, cpu, CpuService::Required)
    }

    /// Reuses one request while promoting selected demand or withdrawing it.
    pub(in crate::application::login_model) fn poll_for(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
        service: CpuService,
    ) -> Result<Option<Arc<GlueBackdropAssets>>, Arc<RuntimeGlueModelError>> {
        self.collect_finished();
        if let Some(result) = self.ready.get(path) {
            return result.clone().map(Some);
        }
        if let Some(pending) = &self.pending {
            // One archive owner serializes these requests. Even a different
            // path is a prerequisite for freeing that owner for selected demand.
            if let Some(demand) = &pending.model_demand {
                demand.set_service(service);
            } else {
                pending.task.set_service(service);
            }
        }
        if let Some(waiting) = &self.waiting {
            waiting.request.set_service(service);
        }
        if self.pending.is_none() {
            match self.submit(path, cpu, service) {
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
    pub(in crate::application::login_model) fn load_blocking(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
    ) -> BackdropResult {
        self.collect_finished();
        loop {
            if let Some(result) = self.ready.get(path) {
                return result.clone();
            }
            if self.pending.is_none() {
                if let Some(waiting) = &self.waiting {
                    // This is the explicit startup coordinator boundary, not a worker wait.
                    waiting.request.set_service(CpuService::Required);
                    let _published = waiting.request.wait();
                }
                self.submit(path, cpu, CpuService::Required)?;
            }
            self.finish_active();
        }
    }

    /// Reports archive work still owned by this scheduler.
    pub(in crate::application::login_model) fn has_pending(&self) -> bool {
        self.pending.is_some() || self.waiting.is_some()
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

    /// Stores the result under its original path, even after the selected scene changes.
    fn finish_active(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if let Some(demand) = &pending.model_demand {
            demand.set_service(CpuService::Required);
        }
        let result = match pending.task.join() {
            Ok(completion) => {
                self.assets = Some(completion.assets);
                completion.result
            }
            Err(error) => Err(Arc::new(RuntimeGlueModelError::from(error))),
        };
        self.ready.insert(pending.path, result);
    }
}

impl Drop for GlueBackdropLoader {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            // Only CPU-owned archive objects survive this join. No task is
            // abandoned when a client closes before its backdrop becomes ready.
            if let Some(demand) = &pending.model_demand {
                demand.set_service(CpuService::Required);
            }
            pending.task.cancel();
            let _completed = pending.task.join();
        }
    }
}
