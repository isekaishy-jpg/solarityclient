//! Reserved finite service ownership and cold/resumable submission facades.

mod once;
mod publication;
mod steps;

use super::CpuService;
use super::dispatch::Dispatch;
use super::worker::WorkerLease;
use std::sync::Arc;

/// One reserved CPU task slot, borrowing the executor until submission.
/// Its lease also counts toward the running-plus-queued capacity bound.
pub struct CpuTaskPermit<'executor> {
    pool: &'executor Arc<Dispatch>,
    service: CpuService,
    execution: crate::CpuServiceExecution,
    lease: WorkerLease,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
    control: Arc<super::task::TaskControl>,
}

impl<'executor> CpuTaskPermit<'executor> {
    /// Binds admission to its service bucket before caller input ownership moves.
    pub(super) fn new(
        pool: &'executor Arc<Dispatch>,
        lease: WorkerLease,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
        service: CpuService,
        budget: &crate::CpuStorageBudget,
    ) -> Result<Self, crate::CpuError> {
        let control = super::task::TaskControl::reserve(budget)?;
        Ok(Self {
            pool,
            lease,
            notifier,
            service,
            execution: crate::CpuServiceExecution::Bulk,
            control,
        })
    }

    /// Declares execution eligibility before ownership moves. Finite operations
    /// must provide bounded nonblocking turns, including their captured cleanup.
    /// The default bulk classification remains appropriate for archive/codec I/O.
    #[must_use]
    pub fn with_execution(mut self, execution: crate::CpuServiceExecution) -> Self {
        self.execution = execution;
        self
    }
}
