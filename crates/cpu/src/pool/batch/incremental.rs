//! Incremental owned admission preserves handles while ready cost changes.

use super::{FrameBatch, FrameJob};
use crate::CpuError;

impl<T: Send + 'static> FrameBatch<T> {
    /// Publishes an independent job under the epoch's reserved limits.
    /// # Errors
    /// Rejection leaves the input with the caller.
    pub fn push(&mut self, job: &mut Option<T>) -> Result<FrameJob<T>, CpuError> {
        self.push_after(job, &[])
    }

    /// Publishes after explicit predecessors. Completion and registration share
    /// one lock, so a terminal parent can neither strand nor double-enqueue a child.
    /// # Errors
    /// Stale/duplicate parents, closed admission or exhausted node/edge capacity
    /// preserve the input. Only earlier nodes in this epoch may be predecessors.
    pub fn push_after(
        &mut self,
        job: &mut Option<T>,
        parents: &[FrameJob<T>],
    ) -> Result<FrameJob<T>, CpuError> {
        self.push_after_with_cost(job, parents, crate::JobCost::default())
    }

    /// Publishes an estimated independent job without changing its output slot.
    /// # Errors
    /// Rejection leaves the input with the caller, as with `push`.
    pub fn push_with_cost(
        &mut self,
        job: &mut Option<T>,
        cost: crate::JobCost,
    ) -> Result<FrameJob<T>, CpuError> {
        self.push_after_with_cost(job, &[], cost)
    }

    /// Publishes an independent group with one queue transaction under an open epoch.
    /// Returns the first node index; `job` exposes stable handles after publication.
    /// All inputs remain with the caller if the complete group cannot be accepted.
    /// # Errors
    /// Rejects inactive/closed epochs, mismatched hints and insufficient node capacity.
    pub fn push_all_with_cost(
        &mut self,
        jobs: &mut impl crate::BatchInputs<T>,
        costs: &[crate::JobCost],
    ) -> Result<usize, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        if !costs.is_empty() && costs.len() != jobs.len() {
            return Err(CpuError::GraphInputCount);
        }
        let mut state = self.core.lock();
        if !state.open {
            return Err(CpuError::BatchClosed);
        }
        if jobs.len() > state.plan.jobs.saturating_sub(state.jobs.len()) {
            return Err(CpuError::BatchCapacity);
        }
        let first = state.jobs.len();
        for (index, job) in jobs.drain_inputs().enumerate() {
            state.append(
                Some(job),
                costs.get(index).copied().unwrap_or_default(),
                std::iter::empty(),
            );
        }
        let raised = self.core.update_cost(&mut state);
        let launch = state.runners_to_launch();
        drop(state);
        if raised {
            self.core.refresh_cost();
        }
        self.core.launch(launch);
        Ok(first)
    }

    /// Adds a cost hint to dependency admission. Priority and eligibility still
    /// take precedence; only simultaneously ready jobs may exchange execution order.
    /// # Errors
    /// Preserves the input on the same identity/capacity errors as `push_after`.
    pub fn push_after_with_cost(
        &mut self,
        job: &mut Option<T>,
        parents: &[FrameJob<T>],
        cost: crate::JobCost,
    ) -> Result<FrameJob<T>, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        if !state.open {
            return Err(CpuError::BatchClosed);
        }
        if job.is_none() {
            return Err(CpuError::InvalidJob);
        }
        if state.jobs.len() == state.plan.jobs
            || parents.len() > state.plan.edges.saturating_sub(state.edge_count)
        {
            return Err(CpuError::BatchCapacity);
        }
        for (position, parent) in parents.iter().enumerate() {
            self.validate(&state, parent)?;
            if parents[..position]
                .iter()
                .any(|earlier| earlier.index == parent.index)
            {
                return Err(CpuError::InvalidJob);
            }
        }
        let index = state.jobs.len();
        state.append(job.take(), cost, parents.iter().map(|parent| parent.index));
        let handle = FrameJob::new(&self.core, state.generation, index);
        let raised = self.core.update_cost(&mut state);
        let launch = state.runners_to_launch();
        drop(state);
        if raised {
            self.core.refresh_cost();
        }
        self.core.launch(launch);
        Ok(handle)
    }
}
