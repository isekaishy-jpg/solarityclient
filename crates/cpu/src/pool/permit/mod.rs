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
    lease: WorkerLease,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

impl<'executor> CpuTaskPermit<'executor> {
    /// Binds admission to its service bucket before caller input ownership moves.
    pub(super) fn new(
        pool: &'executor Arc<Dispatch>,
        lease: WorkerLease,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
        service: CpuService,
    ) -> Self {
        Self {
            pool,
            lease,
            notifier,
            service,
        }
    }
}
