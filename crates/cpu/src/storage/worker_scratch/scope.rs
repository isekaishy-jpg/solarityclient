//! Scoped access checks the executor generation before taking a worker's lane.

use super::CpuWorkerScratch;
use crate::{CpuError, ScratchScope};

impl<T> CpuWorkerScratch<T> {
    /// The higher-ranked callback cannot return a borrow into resettable storage.
    pub(crate) fn scope<R>(
        &self,
        worker: crate::pool::WorkerLane,
        requested: usize,
        operation: impl for<'scope> FnOnce(&mut ScratchScope<'scope, T>) -> R,
    ) -> Result<R, CpuError> {
        if self.core.owner.as_ptr().addr() != worker.owner {
            return Err(CpuError::WorkerScratchOwner);
        }
        if requested > self.capacity() {
            return Err(CpuError::OutputCapacity {
                requested,
                available: self.capacity(),
            });
        }
        let lane = self
            .core
            .lanes
            .get(worker.index)
            .ok_or(CpuError::WorkerScratchOwner)?;
        let mut loan = lane.take(requested)?;
        let mut scope = loan.scratch().scope();
        Ok(operation(&mut scope))
    }
}
