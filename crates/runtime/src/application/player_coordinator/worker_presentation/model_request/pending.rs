//! Typed appearance completion after source readiness, with unconditional cache return.

use super::super::AppearanceCompletion;
use super::work::{AppearanceBank, ModelInput, Prepare};
use solarity_asset::{M2LoadDependency, M2LoadRequest};
use solarity_cpu::{
    CpuError, CpuExecutor, CpuService, CpuStorageClass, CpuTask, JobOutcome, LoadBatch,
};

/// Both paths return source and cache ownership to their admitted appearance owner.
pub(in crate::application::player_coordinator) enum AppearanceTask<T: Send + 'static> {
    Direct {
        task: CpuTask<AppearanceCompletion<T>>,
        demand: Option<M2LoadRequest>,
    },
    Dependent {
        task: Box<DependentTask<T>>,
        demand: M2LoadRequest,
    },
}

/// The owned loading phase retains its bank while shared source readiness is pending.
pub(in crate::application::player_coordinator) struct DependentTask<T: Send + 'static> {
    batch: LoadBatch<DependentWork<T>>,
    jobs: Vec<DependentWork<T>>,
}

/// One slot carries every input through execution, cancellation and source failure.
struct DependentWork<T: Send + 'static> {
    bank: Option<AppearanceBank>,
    prepare: Option<Prepare<T>>,
    dependency: M2LoadDependency,
    completion: Option<AppearanceCompletion<T>>,
}

impl<T: Send + 'static> DependentWork<T> {
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

impl<T: Send + 'static> AppearanceTask<T> {
    pub(in crate::application::player_coordinator) fn is_finished(&self) -> bool {
        match self {
            Self::Direct { task, .. } => task.is_finished(),
            Self::Dependent { task, .. } => task.batch.is_finished(),
        }
    }

    /// Withdraws this appearance without overriding another owner's source demand.
    pub(in crate::application::player_coordinator) fn retire(&mut self) {
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
        bank: &mut Option<AppearanceBank>,
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
    pub(in crate::application::player_coordinator) fn join(
        self,
    ) -> Result<AppearanceCompletion<T>, CpuError> {
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
