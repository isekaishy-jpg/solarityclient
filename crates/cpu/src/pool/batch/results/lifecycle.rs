//! Cancellation and disposal retain unconditional ownership through terminal return.

use super::{CpuError, FrameBatch, FrameJob, JobOutcome, Status};

impl<T: Send + 'static> FrameBatch<T> {
    /// Cancels unstarted nodes immediately; running state stays with its kernel
    /// until return. Transitive successors retain inputs without executing.
    /// # Errors
    /// Reports inactive or stale identities. Terminal cancellation is idempotent.
    pub fn cancel(&mut self, handle: &FrameJob<T>) -> Result<(), CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        self.validate(&state, handle)?;
        match state.nodes[handle.index].status {
            Status::Terminal(_) => return Ok(()),
            Status::Running => state.nodes[handle.index].cancel_requested = true,
            Status::Ready | Status::Waiting => state.complete(handle.index, JobOutcome::Cancelled),
        }
        self.core.update_cost(&mut state);
        let notifier = state.notifier.clone();
        drop(state);
        self.core.ready.notify_all();
        if let Some(notifier) = notifier {
            notifier.notify();
        }
        Ok(())
    }
}

impl<T: Send + 'static> Drop for FrameBatch<T> {
    fn drop(&mut self) {
        let epoch = self.core.lock().generation;
        let owner: std::sync::Arc<dyn crate::pool::epochs::EpochOwner> = self.core.clone();
        owner.stop(epoch);
        // Queued/running dispatch records retain Core and its owned inputs until
        // terminal publication. Executor admission still counts that work. A
        // dropped consumer cannot park its worker waiting for another worker;
        // callers needing inputs back use explicit reclaim before dropping.
    }
}
