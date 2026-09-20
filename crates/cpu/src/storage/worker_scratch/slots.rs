//! A worker takes its temporary container while releasing the transfer guard.

use crate::{CpuError, CpuScratch, CpuStorageBudget, CpuStorageClass};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// One physical worker owns at most one synchronous loan from an operation pool.
pub(super) struct Lane<T> {
    pub(super) storage: Mutex<Option<CpuScratch<T>>>,
    peak: AtomicUsize,
}

impl<T> Lane<T> {
    /// Reserves before dispatch; neither acquisition nor the body grows storage.
    pub(super) fn new(
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        capacity: usize,
    ) -> Result<Self, CpuError> {
        let mut scratch = CpuScratch::default();
        scratch.reserve(budget, class, capacity)?;
        Ok(Self {
            storage: Mutex::new(Some(scratch)),
            peak: AtomicUsize::new(0),
        })
    }

    pub(super) fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }

    /// No contention is expected: contexts cannot leave their physical worker.
    pub(super) fn take(&self, requested: usize) -> Result<Loan<'_, T>, CpuError> {
        let scratch = self
            .storage
            .try_lock()
            .map_err(|_| CpuError::StateUnavailable)?
            .take()
            .ok_or(CpuError::WorkerScratchBorrowed)?;
        // Only this physical lane writes; readers may observe it diagnostically.
        if requested > self.peak.load(Ordering::Relaxed) {
            self.peak.store(requested, Ordering::Relaxed);
        }
        Ok(Loan {
            lane: self,
            scratch: Some(scratch),
        })
    }
}

/// The return guard survives the operation's unwind boundary and owns no lock.
pub(super) struct Loan<'lane, T> {
    lane: &'lane Lane<T>,
    scratch: Option<CpuScratch<T>>,
}

impl<T> Loan<'_, T> {
    pub(super) fn scratch(&mut self) -> &mut CpuScratch<T> {
        self.scratch
            .as_mut()
            .unwrap_or_else(|| unreachable!("live scratch loan owns its container"))
    }
}

impl<T> Drop for Loan<'_, T> {
    fn drop(&mut self) {
        // No user code runs under this guard. A second live loan is prohibited,
        // and replacement versions never mutate these slots.
        let mut slot = self
            .lane
            .storage
            .try_lock()
            .unwrap_or_else(|_| unreachable!("scratch return owns its worker lane"));
        debug_assert!(slot.is_none());
        *slot = self.scratch.take();
    }
}
