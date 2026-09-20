//! Native wait scopes exist only around a condition-variable wait and reacquisition.

use super::{Core, CpuError, FrameBatch, FrameJob, Gate, JobOutcome, State, Status};
use crate::pool::observation::ObservedGuard;

/// A job wait identifies its node; phase retirement waits for all publication.
#[derive(Clone, Copy)]
pub(super) enum WaitTarget {
    Result(usize),
    Phase,
}

impl<T> Core<T> {
    /// Called with a validated pending target. The mutex and native condition wait
    /// preserve the existing no-lost-wakeup protocol. Each retry is a separate
    /// native attempt, including spurious wakeups; no consumer work enters it.
    pub(super) fn wait_for_change<'a>(
        &'a self,
        state: ObservedGuard<'a, State<T>>,
        target: WaitTarget,
    ) -> ObservedGuard<'a, State<T>> {
        state.trace.link("cpu.phase.wait_need");
        let _origin = state.trace.enter();
        let mut profile = match (state.service.is_some(), target) {
            (false, WaitTarget::Result(_)) => solarity_profiling::profile!("cpu.frame.result_wait"),
            (true, WaitTarget::Result(_)) => solarity_profiling::profile!("cpu.load.result_wait"),
            (false, WaitTarget::Phase) => solarity_profiling::profile!("cpu.frame.reclaim_wait"),
            (true, WaitTarget::Phase) => solarity_profiling::profile!("cpu.load.reclaim_wait"),
        };
        // A constant-time admission snapshot explains why progress is pending.
        // 0=phase work, 1=external gate, 2=node dependency, 3=ready queue,
        // 4=running kernel, 5=terminal publication. These are not durations.
        let owner = match target {
            WaitTarget::Result(index) => index as u64 + 1,
            WaitTarget::Phase => 0,
        };
        let reason = if matches!(state.gate, Gate::Pending(_)) {
            1
        } else {
            match target {
                WaitTarget::Result(index) => match state.nodes[index].status {
                    Status::Waiting => 2,
                    Status::Ready => 3,
                    Status::Running => 4,
                    Status::Terminal(_) => 5,
                },
                WaitTarget::Phase if state.finishing => 5,
                WaitTarget::Phase => 0,
            }
        };
        profile.trace_owner(owner, reason);
        state.wait(&self.ready)
    }
}

impl<T: Send + 'static> FrameBatch<T> {
    /// Waits for one terminal outcome without leasing or consuming its payload.
    /// A failed job remains a terminal outcome for the ordered consumer to report.
    /// # Errors
    /// Rejects inactive/stale handles and unfinished waits from CPU workers.
    pub fn wait_for_outcome(&self, handle: &FrameJob<T>) -> Result<JobOutcome, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        self.validate(&state, handle)?;
        if !matches!(state.nodes[handle.index].status, Status::Terminal(_)) {
            if crate::environment::is_worker() {
                return Err(CpuError::WorkerWait);
            }
            drop(state);
            self.require_urgent()?;
            state = self.core.lock();
        }
        loop {
            if let Status::Terminal(outcome) = state.nodes[handle.index].status {
                return Ok(outcome);
            }
            state = self
                .core
                .wait_for_change(state, WaitTarget::Result(handle.index));
        }
    }

    /// Waits for terminal publication/admission release without taking job state.
    /// The producer must already be closed, so no caller can wait on its own append.
    /// # Errors
    /// Rejects an open producer and unfinished waits from CPU workers.
    pub fn wait_until_finished(&self) -> Result<(), CpuError> {
        if !self.active {
            return Ok(());
        }
        let mut state = self.core.lock();
        if state.open {
            return Err(CpuError::BatchOpen);
        }
        if state.lease.is_none() {
            return Ok(());
        }
        if crate::environment::is_worker() {
            return Err(CpuError::WorkerWait);
        }
        drop(state);
        self.require_urgent()?;
        state = self.core.lock();
        while state.lease.is_some() {
            state = self.core.wait_for_change(state, WaitTarget::Phase);
        }
        Ok(())
    }
}
