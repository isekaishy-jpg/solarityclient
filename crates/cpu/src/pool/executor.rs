//! Ownership and lifecycle of the repository CPU executor.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;

use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::pool::task::TaskOutcome;
use crate::pool::worker::SharedExecutorState;
use crate::pool::{CpuError, CpuPoolConfig, CpuPoolSnapshot, CpuTask};

/// The application-owned pool for finite CPU-intensive work.
///
/// This pool is intentionally separate from Tokio. Asset decompression,
/// parsing, visibility work, and other CPU tasks must not occupy network async
/// executor threads. Admission is bounded and non-blocking so an interactive
/// producer can apply its own backpressure policy.
pub struct CpuExecutor {
    pool: Option<ThreadPool>,
    state: Arc<SharedExecutorState>,
    worker_count: usize,
}

impl CpuExecutor {
    /// Builds a private Rayon pool with explicit worker and admission limits.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::PoolBuild`] when worker creation fails.
    pub fn new(config: CpuPoolConfig) -> Result<Self, CpuError> {
        let worker_count = config.worker_count().get();
        let pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|index| format!("solarity-cpu-{index}"))
            .build()
            .map_err(|source| CpuError::PoolBuild {
                message: source.to_string(),
            })?;

        Ok(Self {
            pool: Some(pool),
            state: SharedExecutorState::new(config.max_in_flight()),
            worker_count,
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
        let lease = self.state.reserve()?;
        let pool = self.pool.as_ref().ok_or(CpuError::ShuttingDown)?;
        let (sender, receiver) = sync_channel(1);
        let finished = Arc::new(AtomicBool::new(false));
        let finished_by_worker = Arc::clone(&finished);

        pool.spawn_fifo(move || {
            let outcome = match catch_unwind(AssertUnwindSafe(operation)) {
                Ok(value) => TaskOutcome::Completed(value),
                Err(_panic_payload) => TaskOutcome::Panicked,
            };
            // Completion releases admission before publishing the result. A
            // joining observer must never receive the value while a lifecycle
            // snapshot can still count its work as running or queued.
            drop(lease);
            finished_by_worker.store(true, Ordering::Release);
            let _completion_observed = sender.send(outcome);
        });

        Ok(CpuTask::new(receiver, finished))
    }

    /// Returns the fixed number of private worker threads.
    #[must_use]
    pub const fn worker_count(&self) -> usize {
        self.worker_count
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
            .worker_count
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
        drop(self.pool.take());
        Ok(())
    }
}

impl Drop for CpuExecutor {
    fn drop(&mut self) {
        // Destructors cannot report errors. Normal owners call `shutdown`; this
        // path still closes admission and drains all observable admitted work.
        let _shutdown_result = self.state.stop_and_wait();
        drop(self.pool.take());
    }
}
