//! Pending source consumers remain outside the CPU queue until publication.

use super::{
    BackdropModel, GlueBackdropLoader, PendingBackdrop, RuntimeGlueModelError, WaitingModel,
};
use solarity_asset::{AssetPath, AssetResourceKey, M2Load};
use solarity_cpu::{CpuExecutor, CpuService};
use std::sync::Arc;

impl GlueBackdropLoader {
    /// Joins model identity before dispatch; capacity is reserved before producer/input transfer.
    pub(super) fn submit(
        &mut self,
        path: &AssetPath,
        cpu: &CpuExecutor,
        service: CpuService,
    ) -> Result<(), Arc<RuntimeGlueModelError>> {
        debug_assert!(self.pending.is_none());
        let published = if let Some(waiting) = &self.waiting {
            let Some(result) = waiting.request.poll() else {
                return Ok(());
            };
            match result {
                Ok(model) => Some(model),
                Err(error) => {
                    let waiting = self
                        .waiting
                        .take()
                        .unwrap_or_else(|| unreachable!("observed waiter remains owned"));
                    self.ready.insert(
                        waiting.path,
                        Err(Arc::new(RuntimeGlueModelError::SharedModel(error))),
                    );
                    return Ok(());
                }
            }
        } else {
            None
        };
        if self.waiting.is_none() {
            let key = AssetResourceKey::new(self.namespace, path.clone());
            if let Some(request) = self
                .sources
                .join_pending(&key, service)
                .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))?
            {
                self.waiting = Some(WaitingModel {
                    path: path.clone(),
                    request,
                });
                return Ok(());
            }
        }
        let permit = cpu
            .try_reserve_for(service)
            .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))?;
        let (worker_path, model) = if let Some(model) = published {
            let waiting = self
                .waiting
                .take()
                .unwrap_or_else(|| unreachable!("published model has its waiter"));
            (waiting.path, BackdropModel::Ready(model))
        } else {
            let key = AssetResourceKey::new(self.namespace, path.clone());
            match self
                .sources
                .request_for(&key, service)
                .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))?
            {
                M2Load::Ready(model) => (path.clone(), BackdropModel::Ready(model)),
                M2Load::Producer(producer) => (path.clone(), BackdropModel::Producer(producer)),
                M2Load::Pending(request) => {
                    self.waiting = Some(WaitingModel {
                        path: path.clone(),
                        request,
                    });
                    return Ok(());
                }
            }
        };
        let (model, model_demand) = match model {
            BackdropModel::Producer(producer) => {
                let (producer, demand) = producer
                    .subscribe_owned(service)
                    .map_err(|error| Arc::new(RuntimeGlueModelError::from(error)))?;
                (BackdropModel::Producer(producer), Some(demand))
            }
            ready => (ready, None),
        };
        let assets = self
            .assets
            .take()
            .ok_or_else(|| Arc::new(RuntimeGlueModelError::BackdropWorkerUnavailable))?;
        let result_path = worker_path.clone();
        let shared = crate::application::terrain_coordinator::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let task = permit.submit_resumable_with_context(assets.steps(model, shared));
        if let Some(demand) = &model_demand {
            assert!(
                demand.bind_service(task.service_control()),
                "one producer binds each source task"
            );
        }
        self.pending = Some(PendingBackdrop {
            path: result_path,
            task,
            model_demand,
        });
        Ok(())
    }
}
