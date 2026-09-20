//! Constant-time service eligibility keeps backlog scans off frame publication.

use super::QueuedWork;
use crate::storage::StorageDeque;
use crate::{CpuError, CpuServiceExecution, CpuStorageBudget, CpuStorageClass, CpuStorageKind};

/// One FIFO retains age across demand changes; the finite count is metadata only.
#[derive(Default)]
pub(super) struct ServiceQueue {
    entries: StorageDeque<QueuedWork>,
    finite: usize,
}

impl ServiceQueue {
    /// Reserves every runner before the dispatcher accepts domain ownership.
    pub fn reserve(&mut self, budget: &CpuStorageBudget, capacity: usize) -> Result<(), CpuError> {
        self.entries.reserve(
            budget,
            CpuStorageClass::Required,
            CpuStorageKind::Metadata,
            capacity,
        )
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Eligibility queries occur on frame publication and never walk services.
    pub fn eligible(&self, bulk_available: bool) -> bool {
        self.finite != 0 || (bulk_available && !self.entries.is_empty())
    }

    /// Scanning is necessary only when selecting finite work past blocked bulk.
    pub fn pop_eligible(&mut self, bulk_available: bool) -> Option<QueuedWork> {
        if !self.eligible(bulk_available) {
            return None;
        }
        let work = self
            .entries
            .pop_matching(|work| work.eligible(bulk_available))?;
        self.finite -= usize::from(work.execution() == CpuServiceExecution::Finite);
        Some(work)
    }

    pub fn pop_front(&mut self) -> Option<QueuedWork> {
        let work = self.entries.pop_front()?;
        self.finite -= usize::from(work.execution() == CpuServiceExecution::Finite);
        Some(work)
    }

    pub fn push_back(&mut self, work: QueuedWork) {
        self.finite += usize::from(work.execution() == CpuServiceExecution::Finite);
        self.entries.push_back(work);
    }
}
