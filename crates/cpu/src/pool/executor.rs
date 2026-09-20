//! Ownership and lifecycle of the repository CPU executor.

use std::sync::Arc;

use std::thread::JoinHandle;

use super::dispatch::Dispatch;
use super::permit::CpuTaskPermit;

use crate::pool::worker::SharedExecutorState;
use crate::pool::{CpuError, CpuPoolConfig, CpuPoolSnapshot, CpuService, CpuTask};

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
    pub(super) epochs: super::epochs::Epochs,
    pub(super) frame_capacity: usize,
    pub(super) completion_capacity: usize,
    pub(super) load_epochs: super::epochs::Epochs,
    worker_count: usize,
    execution: super::CpuExecutionPlan,
    storage: crate::CpuStorageBudget,
    pub(super) notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

impl CpuExecutor {
    /// Weak identity prevents scratch bindings from matching a recycled executor address.
    pub(crate) fn worker_owner(&self) -> std::sync::Weak<Dispatch> {
        Arc::downgrade(&self.dispatch)
    }

    /// Builds persistent protected/flexible workers with bounded admission.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::PoolBuild`] when worker creation fails or
    /// [`CpuError::BatchStorage`] when configured queue counts overflow.
    pub fn new(config: CpuPoolConfig) -> Result<Self, CpuError> {
        Self::build(config, None)
    }

    /// Creates a pool whose published completions wake the runtime coordinator.
    /// # Errors
    /// Returns the same worker startup errors as `new`.
    pub fn with_notifier(
        config: CpuPoolConfig,
        notifier: Arc<dyn crate::CoordinatorNotifier>,
    ) -> Result<Self, CpuError> {
        Self::build(config, Some(notifier))
    }

    /// Shares one notifier across the owned producer lifetime.
    fn build(
        config: CpuPoolConfig,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
    ) -> Result<Self, CpuError> {
        let worker_count = config.worker_count().get();
        let storage = crate::CpuStorageBudget::new(config.storage());
        let capacity = config.max_in_flight().get();
        let completion_capacity = capacity.checked_mul(2).ok_or(CpuError::BatchStorage)?;
        let epochs = super::epochs::Epochs::new(capacity, &storage, crate::CpuStorageClass::Frame)?;
        let load_epochs =
            super::epochs::Epochs::new(capacity, &storage, crate::CpuStorageClass::Required)?;
        let (dispatch, workers) =
            Dispatch::start(config.execution(), config.max_in_flight().get(), &storage)?;
        Ok(Self {
            dispatch,
            workers,
            state: SharedExecutorState::new(config.max_in_flight()),
            frame_state: SharedExecutorState::new(config.max_in_flight()),
            epochs,
            load_epochs,
            completion_capacity,
            storage,
            frame_capacity: config.max_in_flight().get(),
            worker_count,
            execution: config.execution(),
            notifier,
        })
    }

    /// Shared capacity accounting remains alive while retained buffers are pinned.
    #[must_use]
    pub fn storage(&self) -> &crate::CpuStorageBudget {
        &self.storage
    }

    /// Creates a main-affinity numeric inbox using this executor's durable wake signal.
    /// The caller reserves its phase metadata with `MainReadyQueue::begin`.
    pub fn main_ready_queue(&self) -> crate::MainReadyQueue {
        crate::MainReadyQueue::new(self.notifier.clone())
    }

    /// Attempts to admit finite CPU work without blocking on queue capacity.
    ///
    /// This entry point submits required service, retaining FIFO ties. Use
    /// `try_submit_for` for speculative work or retirement. Protected workers
    /// never execute these potentially blocking operations.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::AtCapacity`] when the running-plus-queued bound is
    /// full, [`CpuError::ShuttingDown`] after admission closes, or a storage
    /// admission error when the service control cannot be charged.
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
        self.try_reserve_for(CpuService::Required)
    }

    /// Admits a classified service operation without transferring inputs on refusal.
    /// Speculation leaves the final admission slot for required work when capacity
    /// exceeds one. All classes share the configured total task bound.
    /// # Errors
    /// Returns the same lifecycle errors as [`Self::try_reserve`].
    pub fn try_reserve_for(&self, service: CpuService) -> Result<CpuTaskPermit<'_>, CpuError> {
        let lease = self.reserve_service(service)?;
        CpuTaskPermit::new(
            &self.dispatch,
            lease,
            self.notifier.clone(),
            service,
            &self.storage,
        )
    }

    /// Background graphs and ordinary tasks share the same admission bound.
    pub(super) fn reserve_service(
        &self,
        service: CpuService,
    ) -> Result<super::worker::WorkerLease, CpuError> {
        if service == CpuService::Speculative {
            self.state
                .reserve_below(self.frame_capacity.saturating_sub(1).max(1))
        } else {
            self.state.reserve()
        }
    }

    /// Submits required loading, retirement, or optional preparation explicitly.
    /// # Errors
    /// Returns the same admission errors as [`Self::try_reserve_for`].
    pub fn try_submit_for<F, T>(
        &self,
        service: CpuService,
        operation: F,
    ) -> Result<CpuTask<T>, CpuError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Ok(self.try_reserve_for(service)?.submit(operation))
    }

    /// Returns the fixed number of private worker threads.
    #[must_use]
    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Returns the flexible worker count within the configured total budget.
    #[must_use]
    pub const fn background_worker_count(&self) -> usize {
        self.execution.flexible_workers().get()
    }

    /// Returns workers protected from blocking background operations.
    #[must_use]
    pub const fn frame_worker_count(&self) -> usize {
        self.execution.protected_workers()
    }

    /// Reports exactly the execution policy instantiated by this executor.
    #[must_use]
    pub const fn execution_plan(&self) -> super::CpuExecutionPlan {
        self.execution
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
        let speculative_limit = self.execution.bulk_limit().get();
        Ok(snapshot.is_accepting() && snapshot.in_flight() < speculative_limit)
    }

    /// Closes admission, waits for all admitted work, and releases the workers.
    ///
    /// The operation is idempotent. Unresolved external gates are cancelled;
    /// finite running work drains and no task is detached.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::StateUnavailable`] if lifecycle state cannot be
    /// observed consistently.
    pub fn shutdown(&mut self) -> Result<(), CpuError> {
        self.frame_state.close_admission()?;
        self.epochs.stop()?;
        self.state.close_admission()?;
        self.load_epochs.stop()?;
        self.state.stop_and_wait()?;
        self.frame_state.stop_and_wait()?;
        self.dispatch.stop();
        for worker in self.workers.drain(..) {
            worker.join().map_err(|_| CpuError::TaskPanicked)?;
        }
        Ok(())
    }
}

impl Drop for CpuExecutor {
    fn drop(&mut self) {
        // Destructors cannot report errors. Normal owners call `shutdown`; this
        // path still closes admission and drains all observable admitted work.
        let _closed = self.frame_state.close_admission();
        let _stopped = self.epochs.stop();
        let _load_closed = self.state.close_admission();
        let _load_stopped = self.load_epochs.stop();
        let _shutdown_result = self.state.stop_and_wait();
        let _frame_shutdown = self.frame_state.stop_and_wait();
        self.dispatch.stop();
        for worker in self.workers.drain(..) {
            let _joined = worker.join();
        }
    }
}
