//! Shared-source continuations retain the mounted bank across readiness failure.

use super::{
    GameObjectM2Input, GameObjectWorkerCompletion, GameObjectWorkerSource, PendingGeneration,
    ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectPresentation, prepare_on_worker,
};
use solarity_asset::M2LoadDependency;
use solarity_cpu::{
    CpuError, CpuExecutor, CpuService, CpuStorageClass, CpuTask, JobOutcome, LoadBatch,
};

/// Direct producer work and dependent work share one main-owned publication path.
pub(super) enum PendingTask {
    Direct(CpuTask<GameObjectWorkerCompletion>),
    Dependent(Box<DependentTask>),
}
/// The graph retains its source owner until ordered completion is consumed.
pub(super) struct DependentTask {
    batch: LoadBatch<DependentWork>,
    jobs: Vec<DependentWork>,
}
/// Input and completion occupy the same owned slot through all terminal paths.
struct DependentWork {
    source: Option<GameObjectWorkerSource>,
    request: ResourceRequest,
    dependency: M2LoadDependency,
    completion: Option<GameObjectWorkerCompletion>,
    budget: solarity_asset::AssetReadBudget,
}
impl DependentWork {
    /// Called only after shared decoding succeeds. No resource wait occurs here.
    fn prepare(&mut self, context: &solarity_cpu::JobContext<'_>) -> JobOutcome {
        // Keep the complete mounted source with its owner on early withdrawal.
        if context.is_cancelled() {
            return JobOutcome::Cancelled;
        }
        let model = self
            .dependency
            .poll()
            .unwrap_or_else(|| unreachable!("source publication precedes readiness"));
        let source = self
            .source
            .take()
            .unwrap_or_else(|| unreachable!("admitted preparation owns its bank"));
        let completion = match model {
            Ok(model) => prepare_on_worker(
                source,
                &self.request,
                Some(GameObjectM2Input::Ready(model)),
                &self.budget,
            ),
            Err(error) => failed_completion(source, error.into()),
        };
        // Domain errors are published through the normal GameObject policy.
        self.completion = Some(completion);
        JobOutcome::Succeeded
    }
}
/// Failure before useful work returns the mounted cache unchanged.
fn failed_completion(
    source: GameObjectWorkerSource,
    error: RuntimeGameObjectError,
) -> GameObjectWorkerCompletion {
    GameObjectWorkerCompletion {
        worker: match source {
            GameObjectWorkerSource::Ready(worker) => Some(worker),
            GameObjectWorkerSource::Catalog(_) => None,
        },
        result: Err(error),
    }
}
impl PendingTask {
    pub(super) fn is_finished(&self) -> bool {
        match self {
            Self::Direct(task) => task.is_finished(),
            Self::Dependent(task) => task.batch.is_finished(),
        }
    }
    /// A withdrawn consumer cannot start new derived work. Shared producer
    /// priority belongs exclusively to its combined consumer-demand owner.
    pub(super) fn retire(&mut self) {
        match self {
            Self::Direct(_) => {}
            Self::Dependent(task) => task.batch.cancel(),
        }
    }
    /// Reclaims every input before mapping scheduler or source failure to publication.
    pub(super) fn join(self) -> Result<GameObjectWorkerCompletion, CpuError> {
        match self {
            Self::Direct(task) => task.join(),
            Self::Dependent(mut task) => {
                let result = task.batch.reclaim(&mut task.jobs);
                let mut work = task.jobs.pop().ok_or(CpuError::CompletionLost)?;
                if let Some(completion) = work.completion {
                    return Ok(completion);
                }
                let error = match work.dependency.poll() {
                    Some(Err(error)) => error.into(),
                    _ => result.err().unwrap_or(CpuError::CompletionLost).into(),
                };
                let source = work.source.take().ok_or(CpuError::CompletionLost)?;
                Ok(failed_completion(source, error))
            }
        }
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
        let ready = dependency.readiness();
        let mut jobs = vec![DependentWork {
            source: Some(source),
            request: request.clone(),
            dependency,
            completion: None,
            budget: solarity_asset::AssetReadBudget::for_service(
                cpu.storage().clone(),
                CpuService::Required,
            ),
        }];
        let mut batch = LoadBatch::with_context(CpuService::Required, DependentWork::prepare);
        if let Err(error) = batch.start_after(cpu, &mut jobs, &[ready]) {
            let mut work = jobs
                .pop()
                .unwrap_or_else(|| unreachable!("admission refusal preserves input"));
            if let Some(GameObjectWorkerSource::Ready(worker)) = work.source.take() {
                self.worker = Some(worker);
            }
            return match error {
                CpuError::AtCapacity { .. } => Ok(()),
                error => Err(error.into()),
            };
        }
        self.model_wait = None;
        self.pending = Some(PendingGeneration {
            request,
            eligible: true,
            task: PendingTask::Dependent(Box::new(DependentTask { batch, jobs })),
            model_demand: Some(waiting),
        });
        Ok(())
    }
}
