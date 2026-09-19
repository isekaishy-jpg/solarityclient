//! Bounded owned epochs, prerequisite readiness and typed result consumption.

mod admission;
mod diagnostics;
mod execution;
mod loading;
pub use loading::LoadBatch;
mod priority;
mod readiness;
mod ready;
mod results;
mod state;
mod template;
mod types;

#[cfg(test)]
#[path = "../../../tests/internal/epoch_registration.rs"]
mod registration_tests;
pub use template::FrameGraphTemplate;

pub use types::{FrameBatchPlan, FrameJob, FramePriority, JobOutcome};

use super::dispatch::{Work, WorkClass};
use super::{CpuError, CpuExecutor};
use state::{Core, Kernel, State};
use std::sync::{Arc, Condvar, Mutex};

/// Reusable typed state with append-only dependencies inside a checked epoch.
/// Every terminal path retains its input; kernels and consumers run outside locks.
pub struct FrameBatch<T: Send + 'static> {
    core: Arc<Core<T>>,
    active: bool,
    // Fixed admission class; per-epoch service identity lives in State.
    service: Option<crate::CpuService>,
}

impl<T: Send + 'static> FrameBatch<T> {
    /// Registers an operation once; ordinary return is successful completion.
    #[must_use]
    pub fn new(operation: fn(&mut T)) -> Self {
        Self::create(Kernel::Ordinary(operation))
    }

    /// Registers a kernel that retains domain results in T and reports whether
    /// successors may run. Domain errors remain owned for ordered reporting.
    #[must_use]
    pub fn with_outcome(operation: fn(&mut T) -> JobOutcome) -> Self {
        Self::create(Kernel::Reported(operation))
    }

    /// Allocates the synchronization owner only at batch registration.
    fn create(kernel: Kernel<T>) -> Self {
        Self {
            core: Arc::new(Core {
                urgent: std::sync::atomic::AtomicBool::new(false),
                live_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                state: Mutex::new(State::new(kernel)),
                ready: Condvar::new(),
                completion_port: crate::CompletionPort::empty(),
            }),
            active: false,
            service: None,
        }
    }

    /// Reserves a known independent phase before transferring inputs.
    /// # Errors
    /// Admission, epoch exhaustion and storage errors preserve `jobs`.
    pub fn start(&mut self, cpu: &CpuExecutor, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        self.start_graph(cpu, &FrameGraphTemplate::independent(jobs.len()), jobs, &[])
    }

    /// Reserves node/edge metadata before an incremental producer starts.
    /// # Errors
    /// Returns admission, epoch exhaustion or allocation errors without opening
    /// an epoch. The preceding epoch must first be reclaimed.
    pub fn begin(&mut self, cpu: &CpuExecutor, plan: FrameBatchPlan) -> Result<(), CpuError> {
        self.begin_dependencies(cpu, plan, &[])
    }

    /// Opens a phase behind an external/resource or heterogeneous batch result.
    /// Subscription capacity is reserved before any job input is transferred.
    /// # Errors
    /// Returns ordinary admission errors or stale/exhausted readiness capacity.
    pub fn begin_when(
        &mut self,
        cpu: &CpuExecutor,
        plan: FrameBatchPlan,
        dependency: &crate::ReadyToken,
    ) -> Result<(), CpuError> {
        self.begin_dependencies(cpu, plan, std::slice::from_ref(dependency))
    }

    /// Publishes an independent job under the epoch's reserved limits.
    /// # Errors
    /// Rejection leaves the input with the caller.
    pub fn push(&mut self, job: &mut Option<T>) -> Result<FrameJob<T>, CpuError> {
        self.push_after(job, &[])
    }

    /// Publishes after explicit predecessors. Completion and registration share
    /// one lock, so a terminal parent can neither strand nor double-enqueue a child.
    /// # Errors
    /// Stale/duplicate parents, closed admission or exhausted node/edge capacity
    /// preserve the input. Only earlier nodes in this epoch may be predecessors.
    pub fn push_after(
        &mut self,
        job: &mut Option<T>,
        parents: &[FrameJob<T>],
    ) -> Result<FrameJob<T>, CpuError> {
        self.push_after_with_cost(job, parents, crate::JobCost::default())
    }

    /// Publishes an estimated independent job without changing its output slot.
    /// # Errors
    /// Rejection leaves the input with the caller, as with `push`.
    pub fn push_with_cost(
        &mut self,
        job: &mut Option<T>,
        cost: crate::JobCost,
    ) -> Result<FrameJob<T>, CpuError> {
        self.push_after_with_cost(job, &[], cost)
    }

    /// Adds a cost hint to dependency admission. Priority and eligibility still
    /// take precedence; only simultaneously ready jobs may exchange execution order.
    /// # Errors
    /// Preserves the input on the same identity/capacity errors as `push_after`.
    pub fn push_after_with_cost(
        &mut self,
        job: &mut Option<T>,
        parents: &[FrameJob<T>],
        cost: crate::JobCost,
    ) -> Result<FrameJob<T>, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        if !state.open {
            return Err(CpuError::BatchClosed);
        }
        if job.is_none() {
            return Err(CpuError::InvalidJob);
        }
        if state.jobs.len() == state.plan.jobs
            || parents.len() > state.plan.edges.saturating_sub(state.edge_count)
        {
            return Err(CpuError::BatchCapacity);
        }
        for (position, parent) in parents.iter().enumerate() {
            self.validate(&state, parent)?;
            if parents[..position]
                .iter()
                .any(|earlier| earlier.index == parent.index)
            {
                return Err(CpuError::InvalidJob);
            }
        }
        let index = state.jobs.len();
        state.append(job.take(), cost, parents.iter().map(|parent| parent.index));
        let handle = FrameJob::new(&self.core, state.generation, index);
        let launch = state.runners_to_launch();
        drop(state);
        self.core.launch(launch);
        Ok(handle)
    }

    /// Returns a checked handle for a root admitted by `start`.
    /// # Errors
    /// Reports an inactive epoch or a slot outside the admitted phase.
    pub fn job(&self, index: usize) -> Result<FrameJob<T>, CpuError> {
        let state = self.core.lock();
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        if index >= state.jobs.len() {
            return Err(CpuError::InvalidJob);
        }
        Ok(FrameJob::new(&self.core, state.generation, index))
    }

    /// Closes incremental admission without waiting; registered successors still run.
    pub fn close(&mut self) {
        let mut state = self.core.lock();
        state.open = false;
        drop(state);
        self.core.finish_if_terminal();
    }

    /// Exports this phase's terminal readiness without erasing its typed payload.
    /// Consumers retain their own immutable result leases across this dependency.
    /// # Errors
    /// An inactive batch has no currently admitted completion identity.
    pub fn completion(&self) -> Result<crate::ReadyToken, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        self.core
            .lock()
            .completion
            .clone()
            .ok_or(CpuError::BatchInactive)
    }

    /// Checks identity before touching a recycled slot.
    fn validate(&self, state: &State<T>, handle: &FrameJob<T>) -> Result<(), CpuError> {
        if handle.owner.as_ptr() != Arc::as_ptr(&self.core)
            || handle.generation != state.generation
            || handle.index >= state.jobs.len()
        {
            return Err(CpuError::StaleJob);
        }
        Ok(())
    }
}

impl<T: Send + 'static> Core<T> {
    /// Queue publication follows metadata publication without holding its lock.
    fn launch(self: &Arc<Self>, count: usize) {
        if count == 0 {
            return;
        }
        let (dispatch, service) = {
            let state = self.lock();
            (
                state
                    .dispatch
                    .clone()
                    .unwrap_or_else(|| unreachable!("admitted epoch retains dispatcher")),
                state.service.clone(),
            )
        };
        for _ in 0..count {
            match &service {
                Some(service) => dispatch.push(
                    Work::Loading(Arc::clone(service), self.clone()),
                    WorkClass::Background,
                ),
                None => dispatch.push(Work::Retained(self.clone()), WorkClass::Frame),
            }
        }
    }
}
