//! Checked result leases, cancellation and unconditional state reclamation.

use super::state::{Core, Status};
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
        let _wait = solarity_profiling::profile!("cpu.frame.result_wait");
        loop {
            if let Status::Terminal(outcome) = state.nodes[handle.index].status {
                return Ok(outcome);
            }
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch metadata mutations cannot panic"));
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
        let _wait = solarity_profiling::profile!("cpu.frame.reclaim_wait");
        while state.lease.is_some() {
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch metadata mutations cannot panic"));
        }
        Ok(())
    }

    /// Attempts consumption without waiting or losing a pending handle.
    /// # Errors
    /// Reports stale identity or terminal execution/dependency failure.
    pub fn try_with_result<R>(
        &mut self,
        handle: &FrameJob<T>,
        consume: impl FnOnce(&mut T) -> R,
    ) -> Result<Option<R>, CpuError> {
        self.consume(handle, consume, false)
    }
    /// Waits only at the requested consumption boundary.
    /// # Errors
    /// Reports stale identity, terminal failure or an unfinished worker wait.
    pub fn with_result<R>(
        &mut self,
        handle: &FrameJob<T>,
        consume: impl FnOnce(&mut T) -> R,
    ) -> Result<R, CpuError> {
        self.consume(handle, consume, true)?
            .ok_or(CpuError::InvalidJob)
    }
    /// Leases state outside the lock and returns it on consumer unwind.
    fn consume<R>(
        &mut self,
        handle: &FrameJob<T>,
        consume: impl FnOnce(&mut T) -> R,
        wait: bool,
    ) -> Result<Option<R>, CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let mut state = self.core.lock();
        self.validate(&state, handle)?;
        let index = handle.index;
        if wait && !matches!(state.nodes[index].status, Status::Terminal(_)) {
            if crate::environment::is_worker() {
                return Err(CpuError::WorkerWait);
            }
            drop(state);
            self.require_urgent()?;
            state = self.core.lock();
        }
        let _wait = solarity_profiling::profile!("cpu.frame.result_wait");
        let outcome = loop {
            if let Status::Terminal(outcome) = state.nodes[index].status {
                break outcome;
            }
            if !wait {
                return Ok(None);
            }
            if crate::environment::is_worker() {
                return Err(CpuError::WorkerWait);
            }
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch metadata mutations cannot panic"));
        };
        outcome.result()?;
        let job = state.jobs[index]
            .take()
            .unwrap_or_else(|| unreachable!("terminal job retains state"));
        drop(state);
        let mut lease = ReturnedJob {
            core: &self.core,
            index,
            job: Some(job),
        };
        Ok(Some(consume(lease.job.as_mut().unwrap_or_else(|| {
            unreachable!("consumer lease owns state")
        }))))
    }
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
        let notifier = state.notifier.clone();
        drop(state);
        self.core.ready.notify_all();
        if let Some(notifier) = notifier {
            notifier.notify();
        }
        Ok(())
    }
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
        let _wait = if self.service.is_some() {
            solarity_profiling::profile!("cpu.load.reclaim_wait")
        } else {
            solarity_profiling::profile!("cpu.frame.reclaim_wait")
        };
        while state.lease.is_some() {
            state = self
                .core
                .ready
                .wait(state)
                .unwrap_or_else(|_| unreachable!("batch metadata mutations cannot panic"));
        }
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
        result
    }
}

/// A consumer cannot abandon the sole state lease by unwinding.
struct ReturnedJob<'a, T> {
    core: &'a Core<T>,
    index: usize,
    job: Option<T>,
}
impl<T> Drop for ReturnedJob<'_, T> {
    fn drop(&mut self) {
        self.core.lock().jobs[self.index] = self.job.take();
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
