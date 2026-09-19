//! Checked result observation, native waiting, consumption and lifetime return.

mod consumption;
mod lifecycle;
mod reclamation;
mod waiting;

#[cfg(test)]
#[path = "../../../../tests/internal/batch_wait_trace.rs"]
mod tests;

use super::state::{Core, Gate, State, Status};
use super::{FrameBatch, FrameJob, JobOutcome};
use crate::CpuError;

impl<T: Send + 'static> FrameBatch<T> {
    /// Reports when all kernels and terminal publication have released admission.
    /// An inactive batch has no outstanding work. This does not consume results.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !self.active || self.core.lock().lease.is_none()
    }

    /// Observes terminal status without consuming domain state.
    /// # Errors
    /// Rejects inactive, foreign or recycled identities.
    pub fn outcome(&self, handle: &FrameJob<T>) -> Result<Option<JobOutcome>, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let state = self.core.lock();
        self.validate(&state, handle)?;
        Ok(match state.nodes[handle.index].status {
            Status::Terminal(outcome) => Some(outcome),
            _ => None,
        })
    }
}
