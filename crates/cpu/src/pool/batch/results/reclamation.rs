//! Phase retirement separates actual waiting from ordered state return and cleanup.

use super::waiting::WaitTarget;
use super::{CpuError, FrameBatch, Status};

impl<T: Send + 'static> FrameBatch<T> {
    /// Restores all inputs in admission order, including failed/cancelled state.
    /// # Errors
    /// Reports terminal failure only after returning every input. Workers cannot
    /// reclaim unfinished work that might depend on their own occupied lane.
    pub fn reclaim(&mut self, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        if !self.active {
            return Ok(());
        }
        if crate::environment::is_worker() && self.core.lock().lease.is_some() {
            return Err(CpuError::WorkerWait);
        }
        self.require_urgent()?;
        self.close();
        let mut state = self.core.lock();
        while state.lease.is_some() {
            state = self.core.wait_for_change(state, WaitTarget::Phase);
        }
        // Only terminal state copying/cleanup belongs to reclamation. Readiness
        // checks and native condition-variable waits have already completed.
        state.trace.link("cpu.phase.reclaim_need");
        let _origin = state.trace.enter();
        let mut profile = if self.service.is_some() {
            solarity_profiling::profile!("cpu.load.reclaim")
        } else {
            solarity_profiling::profile!("cpu.frame.reclaim")
        };
        profile.trace_owner(0, state.jobs.len() as u64);
        let result = state
            .nodes
            .iter()
            .find_map(|node| match node.status {
                Status::Terminal(outcome) => outcome.result().err(),
                _ => unreachable!("closed drained graph has no unresolved predecessor"),
            })
            .map_or(Ok(()), Err);
        jobs.extend(
            state
                .jobs
                .drain(..)
                .map(|job| job.unwrap_or_else(|| unreachable!("drained epoch owns every input"))),
        );
        state.clear();
        self.active = false;
        drop(state);
        result
    }
}
