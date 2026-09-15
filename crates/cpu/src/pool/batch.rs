//! Reusable owned job storage with independently consumable results.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use super::dispatch::{Dispatch, ReadyWork, Work, WorkClass};
use super::worker::WorkerLease;
use super::{CpuError, CpuExecutor};

/// A retained operation and its current epoch's owned inputs/results.
struct State<T> {
    jobs: Vec<Option<T>>,
    done: Vec<bool>,
    panicked: Vec<bool>,
    next: usize,
    runners: usize,
    open: bool,
    operation: fn(&mut T),
    lease: Option<WorkerLease>,
    trace: solarity_profiling::TraceContext,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

/// Shared only with this batch's admitted workers, never with domain world state.
struct Core<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

/// Reuses job slots across frames and publishes each result independently.
/// Inputs remain owned and recoverable even if an operation panics. Dropping a
/// batch drains its workers; callers cannot abandon live simulation storage.
pub struct FrameBatch<T: Send + 'static> {
    core: Arc<Core<T>>,
    active: bool,
    stream: Option<(Arc<Dispatch>, usize)>,
}

impl<T: Send + 'static> FrameBatch<T> {
    /// Registers the typed kernel once, outside recurring frame execution.
    #[must_use]
    pub fn new(operation: fn(&mut T)) -> Self {
        Self {
            core: Arc::new(Core {
                state: Mutex::new(State {
                    jobs: Vec::new(),
                    done: Vec::new(),
                    panicked: Vec::new(),
                    next: 0,
                    runners: 0,
                    open: false,
                    operation,
                    lease: None,
                    trace: solarity_profiling::TraceContext::default(),
                    notifier: None,
                }),
                ready: Condvar::new(),
            }),
            active: false,
            stream: None,
        }
    }

    /// Transfers reusable inputs after reserving one bounded batch admission.
    /// # Errors
    /// Returns admission errors without moving inputs, or `BatchActive` if the
    /// previous epoch has not been reclaimed.
    pub fn start(&mut self, cpu: &CpuExecutor, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        if self.active {
            return Err(CpuError::BatchActive);
        }
        let lease = cpu.frame_state.reserve()?;
        let runners = cpu.worker_count().min(jobs.len());
        let mut state = self.core.lock();
        state.jobs.extend(jobs.drain(..).map(Some));
        let count = state.jobs.len();
        state.done.clear();
        state.done.resize(count, false);
        state.panicked.clear();
        state.panicked.resize(count, false);
        state.next = 0;
        state.runners = runners;
        state.trace = solarity_profiling::TraceContext::capture().fork("cpu.frame.request");
        state.notifier = cpu.notifier.clone();
        state.lease = Some(lease);
        if runners == 0 {
            state.lease = None;
        }
        drop(state);
        self.active = true;
        for _ in 0..runners {
            cpu.dispatch
                .push(Work::Retained(self.core.clone()), WorkClass::Frame);
        }
        Ok(())
    }

    /// Reserves an epoch whose ordered producer can publish jobs incrementally.
    /// # Errors
    /// Admission failure leaves the batch and all caller inputs unchanged.
    pub fn begin(&mut self, cpu: &CpuExecutor) -> Result<(), CpuError> {
        if self.active {
            return Err(CpuError::BatchActive);
        }
        let lease = cpu.frame_state.reserve()?;
        let mut state = self.core.lock();
        state.done.clear();
        state.panicked.clear();
        state.next = 0;
        state.open = true;
        state.lease = Some(lease);
        state.trace = solarity_profiling::TraceContext::capture().fork("cpu.frame.request");
        state.notifier = cpu.notifier.clone();
        self.stream = Some((Arc::clone(&cpu.dispatch), cpu.worker_count()));
        self.active = true;
        Ok(())
    }

    /// Publishes one owned input as soon as its ordered dependencies resolve.
    /// # Errors
    /// Invalid epoch leaves `job` with the caller; a successful call takes it.
    pub fn push(&mut self, job: &mut Option<T>) -> Result<usize, CpuError> {
        let Some((dispatch, workers)) = &self.stream else {
            return Err(CpuError::BatchInactive);
        };
        if job.is_none() {
            return Err(CpuError::InvalidJob);
        }
        let mut state = self.core.lock();
        let index = state.jobs.len();
        state.jobs.push(job.take());
        state.done.push(false);
        state.panicked.push(false);
        let wake = state.runners < *workers;
        if wake {
            state.runners += 1;
        }
        drop(state);
        if wake {
            dispatch.push(Work::Retained(self.core.clone()), WorkClass::Frame);
        }
        Ok(index)
    }

    /// Consumes one completed result without joining unrelated jobs. The typed
    /// state returns to its slot even when the consumer unwinds.
    /// # Errors
    /// Reports an invalid slot/epoch or this job's panic.
    pub fn with_result<R>(
        &mut self,
        index: usize,
        consume: impl FnOnce(&mut T) -> R,
    ) -> Result<R, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        if index >= state.jobs.len() {
            return Err(CpuError::InvalidJob);
        }
        if crate::environment::is_worker() && !state.done[index] {
            return Err(CpuError::WorkerWait);
        }
        let _wait = solarity_profiling::profile!("cpu.frame.result_wait");
        while !state.done[index] {
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch state mutations cannot panic"));
        }
        if state.panicked[index] {
            return Err(CpuError::TaskPanicked);
        }
        let job = state.jobs[index]
            .take()
            .unwrap_or_else(|| unreachable!("completed job retains its owned state"));
        drop(state);
        let mut lease = ReturnedJob {
            core: &self.core,
            index,
            job: Some(job),
        };
        Ok(consume(lease.job.as_mut().unwrap_or_else(|| {
            unreachable!("consumer lease retains its state")
        })))
    }

    /// Restores all inputs and reusable outputs, including panicked operations.
    /// # Errors
    /// Reports worker panic only after returning every owned state to `jobs`.
    pub fn reclaim(&mut self, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        if !self.active {
            return Ok(());
        }
        let mut state = self.core.lock();
        state.open = false;
        if state.runners == 0 {
            state.lease = None;
        }
        self.stream = None;
        let _wait = solarity_profiling::profile!("cpu.frame.reclaim_wait");
        while state.runners != 0 {
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch state mutations cannot panic"));
        }
        let panicked = state.panicked.iter().any(|value| *value);
        jobs.extend(
            state.jobs.drain(..).map(|job| {
                job.unwrap_or_else(|| unreachable!("drained batch owns every job state"))
            }),
        );
        self.active = false;
        if panicked {
            Err(CpuError::TaskPanicked)
        } else {
            Ok(())
        }
    }
}

impl<T> Core<T> {
    /// State is never locked across a domain kernel or consumer callback.
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state
            .lock()
            .unwrap_or_else(|_| unreachable!("batch state mutations cannot panic"))
    }
}

impl<T: Send> ReadyWork for Core<T> {
    fn run(&self) {
        loop {
            let (index, mut job, operation, trace) = {
                let mut state = self.lock();
                if state.next == state.jobs.len() {
                    state.runners -= 1;
                    if state.runners == 0 && !state.open {
                        state.lease = None;
                    }
                    self.ready.notify_all();
                    return;
                }
                let index = state.next;
                state.next += 1;
                (
                    index,
                    state.jobs[index]
                        .take()
                        .unwrap_or_else(|| unreachable!("each job is claimed once")),
                    state.operation,
                    state.trace,
                )
            };
            let _trace = trace.enter();
            let _execution = solarity_profiling::profile!("cpu.frame.execute");
            // Keep the owned state outside the unwind closure so failure cannot
            // destroy particle history, result pages or input resource pins.
            let failed = catch_unwind(AssertUnwindSafe(|| operation(&mut job))).is_err();
            let mut state = self.lock();
            state.jobs[index] = Some(job);
            state.panicked[index] = failed;
            state.done[index] = true;
            let notifier = state.notifier.clone();
            drop(state);
            self.ready.notify_all();
            if let Some(notifier) = notifier {
                notifier.notify();
            }
        }
    }
}

/// Returns state after consumer unwind without running user code under the lock.
struct ReturnedJob<'a, T> {
    core: &'a Core<T>,
    index: usize,
    job: Option<T>,
}

impl<T> Drop for ReturnedJob<'_, T> {
    fn drop(&mut self) {
        self.core.lock().jobs[self.index] = self.job.take();
    }
}

impl<T: Send + 'static> Drop for FrameBatch<T> {
    fn drop(&mut self) {
        let mut state = self.core.lock();
        state.open = false;
        if state.runners == 0 {
            state.lease = None;
        }
        while state.runners != 0 {
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch state mutations cannot panic"));
        }
    }
}
