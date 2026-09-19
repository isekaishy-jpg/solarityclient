//! Shared primary sources gate appearance work without occupying a waiting worker.

use super::super::worker_presentation::PopulationWorkerCompletion;
use super::super::{
    GlueCharacterWorkerCache, RuntimePlayerError, RuntimePlayerPresentation,
    RuntimePlayerSharedCatalogs, with_worker_presentation,
};
use super::{PopulationStage, PreparedPopulation};
use solarity_asset::{
    ArchiveCatalog, DecodedM2Model, M2LoadDependency, M2LoadProducer, M2LoadRequest, ResourceLease,
};
use solarity_cpu::{
    CpuError, CpuExecutor, CpuService, CpuStorageClass, CpuTask, JobOutcome, LoadBatch,
};
use solarity_rendering::CharacterComponentTextureLevel;

/// Main selects immutable appearance inputs; the worker owns the preparation call.
pub(super) type Prepare<T> = Box<
    dyn FnOnce(
            &mut RuntimePlayerPresentation,
            ResourceLease<DecodedM2Model>,
        ) -> Result<T, RuntimePlayerError>
        + Send,
>;

/// One exclusive archive/derived-cache bank returns independently of source success.
pub(super) struct PopulationBank {
    pub(super) catalog: ArchiveCatalog,
    pub(super) catalogs: RuntimePlayerSharedCatalogs,
    pub(super) level: CharacterComponentTextureLevel,
    pub(super) cache: GlueCharacterWorkerCache,
}

/// A reserved producer owns decoding; a ready lease avoids any second model lookup.
pub(super) enum ModelInput {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
}

impl PopulationBank {
    /// Runs archive work outside request metadata locks and restores the bank on error.
    pub(super) fn prepare<T>(
        mut self,
        model: ModelInput,
        prepare: Prepare<T>,
    ) -> PopulationWorkerCompletion<T> {
        // A producer must publish the actual mount error to its joiners, not
        // merely disappear and turn it into an abandoned-producer diagnosis.
        if let Err(error) = self.cache.mount(&self.catalog) {
            let error = match model {
                ModelInput::Producer(producer) => {
                    let error = solarity_asset::M2LoadError::Asset(std::sync::Arc::new(error));
                    producer.fail(error.clone());
                    RuntimePlayerError::from(error)
                }
                ModelInput::Ready(_) => RuntimePlayerError::from(error),
            };
            return self.failed(error);
        }
        let result = with_worker_presentation(
            self.catalog,
            self.catalogs,
            self.level,
            &mut self.cache,
            |presentation| {
                let model = match model {
                    ModelInput::Ready(model) => model,
                    ModelInput::Producer(producer) => {
                        producer.load(&mut presentation.assets.borrow_mut())?
                    }
                };
                prepare(presentation, model)
            },
        );
        PopulationWorkerCompletion {
            cache: self.cache,
            result,
        }
    }

    /// A cancelled or failed prerequisite leaves all worker-local caches owned.
    fn failed<T>(self, error: RuntimePlayerError) -> PopulationWorkerCompletion<T> {
        PopulationWorkerCompletion {
            cache: self.cache,
            result: Err(error),
        }
    }
}

/// Both paths publish through the same exact-lifetime population owner.
pub(super) enum PendingTask<T: PreparedPopulation> {
    Direct {
        task: CpuTask<PopulationWorkerCompletion<T>>,
        demand: Option<M2LoadRequest>,
    },
    Dependent {
        task: Box<DependentTask<T>>,
        demand: M2LoadRequest,
    },
}

/// The owned loading phase retains its bank while shared source readiness is pending.
pub(super) struct DependentTask<T: PreparedPopulation> {
    batch: LoadBatch<DependentWork<T>>,
    jobs: Vec<DependentWork<T>>,
}

/// One slot carries every input through execution, cancellation and source failure.
struct DependentWork<T: PreparedPopulation> {
    bank: Option<PopulationBank>,
    prepare: Option<Prepare<T>>,
    dependency: M2LoadDependency,
    completion: Option<PopulationWorkerCompletion<T>>,
}

impl<T: PreparedPopulation> DependentWork<T> {
    /// The readiness gate guarantees publication before any worker begins this call.
    fn run(&mut self) -> JobOutcome {
        let model = self
            .dependency
            .poll()
            .unwrap_or_else(|| unreachable!("source publication precedes readiness"));
        let bank = self
            .bank
            .take()
            .unwrap_or_else(|| unreachable!("admitted appearance owns its bank"));
        let prepare = self
            .prepare
            .take()
            .unwrap_or_else(|| unreachable!("appearance runs once"));
        self.completion = Some(match model {
            Ok(model) => bank.prepare(ModelInput::Ready(model), prepare),
            Err(error) => bank.failed(error.into()),
        });
        JobOutcome::Succeeded
    }
}

impl<T: PreparedPopulation> PendingTask<T> {
    pub(super) fn is_finished(&self) -> bool {
        match self {
            Self::Direct { task, .. } => task.is_finished(),
            Self::Dependent { task, .. } => task.batch.is_finished(),
        }
    }

    /// Withdraws this appearance without overriding another owner's source demand.
    pub(super) fn retire(&mut self) {
        match self {
            Self::Direct { task, demand } => match demand {
                Some(demand) => demand.set_service(CpuService::Retirement),
                None => task.set_service(CpuService::Retirement),
            },
            Self::Dependent { task, demand } => {
                demand.set_service(CpuService::Retirement);
                task.batch.cancel();
            }
        }
    }

    /// Admission failure preserves the bank; success transfers it into the phase.
    pub(super) fn dependent(
        cpu: &CpuExecutor,
        bank: &mut Option<PopulationBank>,
        demand: M2LoadRequest,
        prepare: Prepare<T>,
    ) -> Result<Self, CpuError> {
        let dependency = demand.dependency(cpu.storage(), CpuStorageClass::Required)?;
        let ready = dependency.readiness();
        let mut jobs = vec![DependentWork {
            bank: bank.take(),
            prepare: Some(prepare),
            dependency,
            completion: None,
        }];
        let mut batch = LoadBatch::new(CpuService::Required, DependentWork::run);
        if let Err(error) = batch.start_after(cpu, &mut jobs, &[ready]) {
            *bank = Some(
                jobs.pop()
                    .and_then(|work| work.bank)
                    .unwrap_or_else(|| unreachable!("refused admission preserves its bank")),
            );
            return Err(error);
        }
        Ok(Self::Dependent {
            task: Box::new(DependentTask { batch, jobs }),
            demand,
        })
    }

    /// Restores source/cache ownership before reporting any terminal pipeline error.
    pub(super) fn join(self) -> Result<PopulationWorkerCompletion<T>, CpuError> {
        match self {
            Self::Direct { task, .. } => task.join(),
            Self::Dependent { mut task, .. } => {
                let outcome = task.batch.reclaim(&mut task.jobs);
                let mut work = task.jobs.pop().ok_or(CpuError::CompletionLost)?;
                if let Some(completion) = work.completion {
                    return Ok(completion);
                }
                // A kernel panic can retire its moved bank during unwind. Keep
                // that execution failure rather than misreporting a lost result.
                let Some(bank) = work.bank.take() else {
                    return Err(outcome.err().unwrap_or(CpuError::CompletionLost));
                };
                let error = match work.dependency.poll() {
                    Some(Err(error)) => error.into(),
                    _ => outcome.err().unwrap_or(CpuError::CompletionLost).into(),
                };
                Ok(bank.failed(error))
            }
        }
    }
}

impl<T: PreparedPopulation> PopulationStage<T> {
    /// Only unfinished CPU work needs withdrawal; warming output is already owned.
    pub(super) fn retire(&mut self) {
        match self {
            Self::Preparing(task) => task.retire(),
            Self::Warming { .. } => {}
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        match self {
            Self::Preparing(task) => task.is_finished(),
            Self::Warming { .. } => true,
        }
    }

    /// Used only after completion, including when an obsolete owner must retire.
    pub(super) fn into_result(
        self,
        cache: &mut Option<GlueCharacterWorkerCache>,
    ) -> Result<Result<T, RuntimePlayerError>, CpuError> {
        match self {
            Self::Preparing(task) => {
                let completion = task.join()?;
                *cache = Some(completion.cache);
                Ok(completion.result)
            }
            Self::Warming { resident, .. } => Ok(Ok(resident)),
        }
    }
}
