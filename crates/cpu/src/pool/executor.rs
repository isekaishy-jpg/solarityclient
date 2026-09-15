//! Ownership and lifecycle of the repository CPU executor.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::time::Instant;

use rayon::{ThreadPool, ThreadPoolBuilder};

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
    pool: Option<ThreadPool>,
    /// Synchronous frame work cannot steal archive jobs while joining children.
    frame_pool: Option<ThreadPool>,
    state: Arc<SharedExecutorState>,
    worker_count: usize,
    background_workers: usize,
}

impl CpuExecutor {
    /// Builds a private Rayon pool with explicit worker and admission limits.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::PoolBuild`] when worker creation fails.
    pub fn new(config: CpuPoolConfig) -> Result<Self, CpuError> {
        let worker_count = config.worker_count().get();
        let background_workers = worker_count.div_ceil(2);
        let pool = ThreadPoolBuilder::new()
            .num_threads(background_workers)
            .thread_name(|index| format!("solarity-cpu-{index}"))
            .build()
            .map_err(|source| CpuError::PoolBuild {
                message: source.to_string(),
            })?;
        let frame_pool = if worker_count > 1 {
            Some(
                ThreadPoolBuilder::new()
                    .num_threads(worker_count - background_workers)
                    .thread_name(|index| format!("solarity-frame-{index}"))
                    .build()
                    .map_err(|source| CpuError::PoolBuild {
                        message: source.to_string(),
                    })?,
            )
        } else {
            None
        };

        Ok(Self {
            pool: Some(pool),
            frame_pool,
            state: SharedExecutorState::new(config.max_in_flight()),
            worker_count,
            background_workers,
        })
    }

    /// Attempts to admit finite CPU work without blocking on queue capacity.
    ///
    /// External submissions use Rayon's FIFO injection path so older queued
    /// work is not deliberately buried beneath newer submissions. Rayon may
    /// still steal work between workers and no total execution order is
    /// promised.
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
        let pool = self.pool.as_ref().ok_or(CpuError::ShuttingDown)?;
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

    /// Returns archive/job workers within the configured total thread budget.
    #[must_use]
    pub const fn background_worker_count(&self) -> usize {
        self.background_workers
    }

    /// Returns workers reserved exclusively for joined frame computation.
    #[must_use]
    pub const fn frame_worker_count(&self) -> usize {
        self.worker_count - self.background_workers
    }

    /// Joins borrowed frame work without entering the background job scheduler.
    /// A one-worker configuration executes on the caller, preserving the total
    /// worker budget. No archive job can run while a frame worker joins a child.
    /// The borrow prevents shutdown until every frame item has completed.
    ///
    /// # Errors
    /// Returns [`CpuError::ShuttingDown`] after shutdown or
    /// [`CpuError::TaskPanicked`] after all item borrows have ended on a panic.
    pub fn for_each_frame<T, F>(&self, items: &mut [T], operation: F) -> Result<(), CpuError>
    where
        T: Send,
        F: Fn(&mut T) + Send + Sync,
    {
        if self.pool.is_none() {
            return Err(CpuError::ShuttingDown);
        }
        super::frame::execute(self.frame_pool.as_ref(), items, operation)
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
        let speculative_limit = self
            .background_workers
            .saturating_sub(1)
            .max(1)
            .min(snapshot.max_in_flight().get());
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
        drop(self.frame_pool.take());
        drop(self.pool.take());
        Ok(())
    }
}

/// One reserved CPU task slot, borrowing the executor until submission.
/// Its lease also counts toward the running-plus-queued capacity bound.
pub struct CpuTaskPermit<'executor> {
    pool: &'executor ThreadPool,
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
        pool.spawn_fifo(move || {
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
            finished_by_worker.store(true, Ordering::Release);
            let _completion_observed = sender.send(outcome);
        });
        CpuTask::new(receiver, finished, trace)
    }
}

impl Drop for CpuExecutor {
    fn drop(&mut self) {
        // Destructors cannot report errors. Normal owners call `shutdown`; this
        // path still closes admission and drains all observable admitted work.
        let _shutdown_result = self.state.stop_and_wait();
        drop(self.frame_pool.take());
        drop(self.pool.take());
    }
}
