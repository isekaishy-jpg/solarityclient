//! Resource-gated owned loading phases share graph readiness, never frame eligibility.

use super::state::{Gate, Status};
use super::{FrameBatch, FrameGraphTemplate, JobOutcome};
use crate::{CpuError, CpuExecutor, CpuService, CpuServiceControl, ReadyToken};
use std::sync::Arc;

/// A bounded background phase. Pending prerequisites consume admission, not a
/// worker. Every terminal path retains inputs for ordered owner reclamation.
/// Blocking asset operations execute only on the flexible service worker.
pub struct LoadBatch<T: Send + 'static> {
    batch: FrameBatch<T>,
}
impl<T: Send + 'static> LoadBatch<T> {
    /// Registers a background kernel and its fixed storage/admission class.
    #[must_use]
    pub fn new(service: CpuService, operation: fn(&mut T) -> JobOutcome) -> Self {
        let mut batch = FrameBatch::with_outcome(operation);
        batch.service = Some(service);
        Self { batch }
    }

    /// Reserves all metadata and subscriptions before transferring any input.
    /// # Errors
    /// Capacity, lifecycle or readiness errors leave every input with the caller.
    pub fn start_after(
        &mut self,
        cpu: &CpuExecutor,
        jobs: &mut Vec<T>,
        dependencies: &[ReadyToken],
    ) -> Result<(), CpuError> {
        self.batch.start_graph(
            cpu,
            &FrameGraphTemplate::independent(jobs.len()),
            jobs,
            dependencies,
        )
    }

    /// Exports terminal readiness without erasing or transferring typed results.
    /// # Errors
    /// An inactive batch has no completion identity.
    pub fn completion(&self) -> Result<ReadyToken, CpuError> {
        self.batch.completion()
    }

    /// Reports whether reclamation can proceed without waiting for a kernel.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.batch.active && self.batch.core.lock().lease.is_none()
    }

    /// Controls this epoch only. Urgency never permits protected-worker execution.
    /// # Errors
    /// An inactive batch has no scheduling identity.
    pub fn service_control(&self) -> Result<CpuServiceControl, CpuError> {
        if !self.batch.active {
            return Err(CpuError::BatchInactive);
        }
        let state = self.batch.core.lock();
        let dispatch = state.dispatch.as_ref().ok_or(CpuError::BatchInactive)?;
        let service = state.service.as_ref().ok_or(CpuError::BatchInactive)?;
        Ok(CpuServiceControl::new(dispatch, Arc::clone(service)))
    }

    /// Withdraws this consumer without cancelling shared prerequisite producers.
    /// Unstarted kernels are suppressed; running kernels finish with owned input.
    /// Reclamation still returns every input. Repeated cancellation is harmless.
    pub fn cancel(&mut self) {
        if !self.batch.active {
            return;
        }
        let mut state = self.batch.core.lock();
        state.open = false;
        for index in 0..state.nodes.len() {
            match state.nodes[index].status {
                Status::Running => state.nodes[index].cancel_requested = true,
                Status::Waiting | Status::Ready => state.complete(index, JobOutcome::Cancelled),
                Status::Terminal(_) => {}
            }
        }
        if matches!(state.gate, Gate::Pending(_)) {
            state.fail_gate();
        }
        state.ready.clear();
        self.batch.core.update_cost(&mut state);
        drop(state);
        self.batch.core.finish_if_terminal();
        self.batch.core.ready.notify_all();
    }

    /// Returns all inputs in admission order, even on failed dependencies or shutdown.
    /// # Errors
    /// Reports terminal failure after returning inputs, or rejects an unfinished worker wait.
    pub fn reclaim(&mut self, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        self.batch.reclaim(jobs)
    }
}
