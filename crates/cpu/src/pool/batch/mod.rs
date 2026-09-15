//! Bounded owned epochs, prerequisite readiness and typed result consumption.

mod execution;
mod results;
mod state;
mod types;

pub use types::{FrameBatchPlan, FrameJob, JobOutcome};

use super::dispatch::{Work, WorkClass};
use super::{CpuError, CpuExecutor};
use state::{Core, Kernel, State};
use std::sync::{Arc, Condvar, Mutex};

/// Reusable typed state with append-only dependencies inside a checked epoch.
/// Every terminal path retains its input; kernels and consumers run outside locks.
pub struct FrameBatch<T: Send + 'static> {
    core: Arc<Core<T>>,
    active: bool,
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
                state: Mutex::new(State::new(kernel)),
                ready: Condvar::new(),
            }),
            active: false,
        }
    }

    /// Reserves a known independent phase before transferring inputs.
    /// # Errors
    /// Admission, epoch exhaustion and storage errors preserve `jobs`.
    pub fn start(&mut self, cpu: &CpuExecutor, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        self.begin(cpu, FrameBatchPlan::new(jobs.len(), 0))?;
        let mut state = self.core.lock();
        for job in jobs.drain(..) {
            state.append(Some(job), &[]);
        }
        state.open = false;
        let launch = state.runners_to_launch();
        state.release_if_terminal();
        drop(state);
        self.core.launch(launch);
        Ok(())
    }

    /// Reserves node/edge metadata before an incremental producer starts.
    /// # Errors
    /// Returns admission, epoch exhaustion or allocation errors without opening
    /// an epoch. The preceding epoch must first be reclaimed.
    pub fn begin(&mut self, cpu: &CpuExecutor, plan: FrameBatchPlan) -> Result<(), CpuError> {
        if self.active {
            return Err(CpuError::BatchActive);
        }
        let lease = cpu.frame_state.reserve()?;
        let mut state = self.core.lock();
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(CpuError::EpochExhausted)?;
        state.reserve(plan)?;
        state.generation = generation;
        state.plan = plan;
        state.open = true;
        state.workers = cpu.worker_count();
        state.dispatch = Some(Arc::clone(&cpu.dispatch));
        state.notifier = cpu.notifier.clone();
        state.trace = solarity_profiling::TraceContext::capture().fork("cpu.frame.request");
        state.lease = Some(lease);
        self.active = true;
        Ok(())
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
        state.append(job.take(), parents);
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
        state.release_if_terminal();
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
        let dispatch = self
            .lock()
            .dispatch
            .clone()
            .unwrap_or_else(|| unreachable!("admitted epoch retains dispatcher"));
        for _ in 0..count {
            dispatch.push(Work::Retained(self.clone()), WorkClass::Frame);
        }
    }
}
