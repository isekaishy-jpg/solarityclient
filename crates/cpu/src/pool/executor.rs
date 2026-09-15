//! Ownership and lifecycle of the repository CPU executor.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::time::Instant;

use std::thread::JoinHandle;

use super::dispatch::{Dispatch, Work, WorkClass};

use crate::pool::task::TaskOutcome;
use crate::pool::worker::{SharedExecutorState, WorkerLease};
use crate::pool::{CpuError, CpuPoolConfig, CpuPoolSnapshot, CpuTask};

/// The application-owned pool for finite CPU-intensive work.
///
/// This pool is intentionally separate from Tokio. Asset decompression,
/// parsing, visibility work, and other CPU tasks must not occupy network async
/// executor threads. Admission is bounded and non-blocking so an interactive
/// producer can apply its own backpressure policy.
pub struct CpuExecutor {
    pub(super) dispatch: Arc<Dispatch>,
    workers: Vec<JoinHandle<()>>,
    state: Arc<SharedExecutorState>,
    pub(super) frame_state: Arc<SharedExecutorState>,
    worker_count: usize,
}

impl CpuExecutor {
    /// Builds persistent protected/flexible workers with bounded admission.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::PoolBuild`] when worker creation fails.
    pub fn new(config: CpuPoolConfig) -> Result<Self, CpuError> {
        let worker_count = config.worker_count().get();
        let (dispatch, workers) = Dispatch::start(worker_count, config.max_in_flight().get())?;
        Ok(Self {
            dispatch,
            workers,
            state: SharedExecutorState::new(config.max_in_flight()),
            frame_state: SharedExecutorState::new(config.max_in_flight()),
            worker_count,
        })
    }

    /// Attempts to admit finite CPU work without blocking on queue capacity.
    ///
    /// External submissions use the flexible worker FIFO queue so older queued
    /// work is not buried beneath newer submissions. Protected workers never
    /// execute these potentially blocking operations.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::AtCapacity`] when the running-plus-queued bound is
    /// full or [`CpuError::ShuttingDown`] after admission closes.
    pub fn try_submit<F, T>(&self, operation: F) -> Result<CpuTask<T>, CpuError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Ok(self.try_reserve()?.submit(operation))
    }

    /// Reserves admission before the caller transfers ownership of task inputs.
    /// Dropping an unused permit immediately returns its capacity. This lets
    /// interactive producers retry saturation without losing captured frames or
    /// already prepared asset state inside a rejected closure.
    ///
    /// # Errors
    /// Returns the same admission errors as [`Self::try_submit`].
    pub fn try_reserve(&self) -> Result<CpuTaskPermit<'_>, CpuError> {
        let pool = &self.dispatch;
        Ok(CpuTaskPermit {
            pool,
            lease: self.state.reserve()?,
        })
    }

    /// Returns the fixed number of private worker threads.
    #[must_use]
    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Returns the flexible worker count within the configured total budget.
    #[must_use]
    pub const fn background_worker_count(&self) -> usize {
        1
    }

    /// Returns workers protected from blocking background operations.
    #[must_use]
    pub const fn frame_worker_count(&self) -> usize {
        self.worker_count.saturating_sub(1)
    }

    /// Returns a lock-consistent executor lifecycle snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::StateUnavailable`] if an internal synchronization
    /// invariant was poisoned.
    pub fn snapshot(&self) -> Result<CpuPoolSnapshot, CpuError> {
        self.state.snapshot()
    }

    /// Reports whether a single-owner scheduler may admit speculative work
    /// while reserving one multi-worker lane for latency-sensitive tasks.
    ///
    /// The result is advisory because workers can finish concurrently. Callers
    /// should query immediately before [`Self::try_submit`] and must still
    /// handle its ordinary capacity errors. A one-worker pool admits one
    /// speculative task because no separate interactive lane can exist.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::StateUnavailable`] when lifecycle state is poisoned.
    pub fn can_admit_speculative(&self) -> Result<bool, CpuError> {
        let snapshot = self.snapshot()?;
        let speculative_limit = 1;
        Ok(snapshot.is_accepting() && snapshot.in_flight() < speculative_limit)
    }

    /// Closes admission, waits for all admitted work, and releases the workers.
    ///
    /// The operation is idempotent. No task is cancelled or detached.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::StateUnavailable`] if lifecycle state cannot be
    /// observed consistently.
    pub fn shutdown(&mut self) -> Result<(), CpuError> {
        self.state.stop_and_wait()?;
        self.frame_state.stop_and_wait()?;
        self.dispatch.stop();
        for worker in self.workers.drain(..) {
            worker.join().map_err(|_| CpuError::TaskPanicked)?;
        }
        Ok(())
    }
}

/// One reserved CPU task slot, borrowing the executor until submission.
/// Its lease also counts toward the running-plus-queued capacity bound.
pub struct CpuTaskPermit<'executor> {
    pool: &'executor Dispatch,
    lease: WorkerLease,
}

impl CpuTaskPermit<'_> {
    /// Transfers the admitted operation to the executor's FIFO queue.
    /// Admission cannot fail after the caller relinquishes its inputs.
    pub fn submit<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let Self { pool, lease } = self;
        let (sender, receiver) = sync_channel(1);
        let finished = Arc::new(AtomicBool::new(false));
        let finished_by_worker = Arc::clone(&finished);
        let epoch = solarity_profiling::generation();
        let queued = (epoch != 0).then(Instant::now);
        let trace = solarity_profiling::TraceContext::capture().fork("cpu.job");
        pool.push(
            Work::Once(Box::new(move || {
                let _trace = trace.enter();
                let _profile = solarity_profiling::profile!("cpu.job.execute");
                if let Some(queued) = queued {
                    static QUEUE: solarity_profiling::Site =
                        solarity_profiling::Site::new("cpu.job.queue_wait", false);
                    QUEUE.cpu_duration(epoch, "", queued.elapsed());
                }
                let outcome = match catch_unwind(AssertUnwindSafe(operation)) {
                    Ok(value) => TaskOutcome::Completed(value),
                    Err(_panic_payload) => TaskOutcome::Panicked,
                };
                // Publish completion only after returning admission capacity.
                drop(lease);
                let _completion_observed = sender.send(outcome);
                finished_by_worker.store(true, Ordering::Release);
            })),
            WorkClass::Background,
        );
        CpuTask::new(receiver, finished, trace)
    }
}

impl Drop for CpuExecutor {
    fn drop(&mut self) {
        // Destructors cannot report errors. Normal owners call `shutdown`; this
        // path still closes admission and drains all observable admitted work.
        let _shutdown_result = self.state.stop_and_wait();
        let _frame_shutdown = self.frame_state.stop_and_wait();
        self.dispatch.stop();
        for worker in self.workers.drain(..) {
            let _joined = worker.join();
        }
    }
}
