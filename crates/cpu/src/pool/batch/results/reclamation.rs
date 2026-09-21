//! Phase retirement separates actual waiting from ordered state return and cleanup.

use super::waiting::WaitTarget;
use super::{CpuError, FrameBatch, Status};

impl<T: Send + 'static> FrameBatch<T> {
    /// Restores all inputs in admission order, including failed/cancelled state.
    /// # Errors
    /// Reports terminal failure only after returning every input. Workers cannot
    /// reclaim unfinished work that might depend on their own occupied lane.
    /// Destination allocation refusal keeps every input in the batch for retry.
    pub fn reclaim(&mut self, jobs: &mut Vec<T>) -> Result<(), CpuError> {
        self.reclaim_into(jobs)
    }

    /// Returns inputs into an already admitted writer without growing its storage.
    /// A Vec destination retains the ordinary fallible growth behavior.
    /// # Errors
    /// Insufficient destination capacity leaves every input in the closed batch for retry.
    /// Terminal job failure is returned only after all inputs have been restored.
    pub fn reclaim_into(&mut self, jobs: &mut impl crate::OutputBuffer<T>) -> Result<(), CpuError> {
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
        jobs.try_reserve_exact(state.jobs.len())?;
        let result = state
            .nodes
            .iter()
            .find_map(|node| match node.status {
                Status::Terminal(outcome) => outcome.result().err(),
                _ => unreachable!("closed drained graph has no unresolved predecessor"),
            })
            .map_or(Ok(()), Err);
        for job in state.jobs.drain(..) {
            jobs.push(job.unwrap_or_else(|| unreachable!("drained epoch owns every input")))
                .unwrap_or_else(|_| {
                    unreachable!("reclamation preflight admitted every returned input")
                });
        }
        state.clear();
        self.active = false;
        drop(state);
        result
    }
}
