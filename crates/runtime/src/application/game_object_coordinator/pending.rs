//! Primary and material source waits retain one owned GameObject bank.
use super::{
    GameObjectM2Input, GameObjectWorkerCompletion, GameObjectWorkerSource, PendingGeneration,
    ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectPresentation,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuService, CpuStorageClass, CpuTask};

/// Every source route uses the same resumable publication path.
pub(super) enum PendingTask {
    Direct(CpuTask<GameObjectWorkerCompletion>),
}
impl PendingTask {
    pub(super) fn is_finished(&self) -> bool {
        let Self::Direct(task) = self;
        task.is_finished()
    }
    pub(super) fn retire(&mut self) {
        let Self::Direct(task) = self;
        task.cancel();
    }
    pub(super) fn join(self) -> Result<GameObjectWorkerCompletion, CpuError> {
        let Self::Direct(task) = self;
        task.join()
    }
}
impl PendingGeneration {
    /// Withdrawal affects this consumer only; source demand still combines all owners.
    pub(super) fn retire(&mut self) {
        self.eligible = false;
        self.task.retire();
        if let Some(demand) = &self.model_demand {
            demand.set_service(CpuService::Retirement);
        }
    }
}
impl RuntimeGameObjectPresentation {
    /// Membership/display publication is the invalidation boundary. An unchanged
    /// frame performs no extra request scan, and surviving shared owners keep work live.
    pub(super) fn retire_unreferenced_pending(&mut self) {
        if let Some(pending) = &mut self.pending
            && pending.eligible
            && !self.instances.iter().any(|instance| {
                instance.resource.is_none()
                    && !instance.failed
                    && instance.request.as_ref() == Some(&pending.request)
            })
        {
            pending.retire();
        }
    }
    /// Reserves a dependent phase only for an existing source producer. Saturation
    /// leaves request ownership and the mounted bank with the coordinator.
    pub(super) fn start_model_dependency(
        &mut self,
        cpu: &CpuExecutor,
        request: ResourceRequest,
    ) -> Result<(), RuntimeGameObjectError> {
        let waiting = self
            .model_wait
            .as_ref()
            .unwrap_or_else(|| unreachable!("observed model dependency exists"))
            .1
            .clone();
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let dependency = waiting.dependency(cpu.storage(), CpuStorageClass::Required)?;
        let source = if let Some(worker) = self.worker.take() {
            GameObjectWorkerSource::Ready(worker)
        } else {
            GameObjectWorkerSource::Catalog(
                self.worker_catalog
                    .as_ref()
                    .ok_or(RuntimeGameObjectError::MissingWorkerCatalog)?
                    .clone(),
            )
        };
        let shared = crate::application::terrain_coordinator::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let task = permit.submit_resumable_with_context(super::worker::model_steps(
            source,
            GameObjectM2Input::Waiting(dependency),
            shared,
        ));
        self.model_wait = None;
        self.pending = Some(PendingGeneration {
            request,
            eligible: true,
            task: PendingTask::Direct(task),
            model_demand: Some(waiting),
        });
        Ok(())
    }
}
