//! Ready outputs lease their payload outside scheduler locks and return on unwind.

use super::waiting::WaitTarget;
use super::{Core, CpuError, FrameBatch, FrameJob, Status};

impl<T: Send + 'static> FrameBatch<T> {
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
            state = self.core.wait_for_change(state, WaitTarget::Result(index));
        };
        outcome.result()?;
        let trace = state.trace;
        let job = state.jobs[index]
            .take()
            .unwrap_or_else(|| unreachable!("terminal job retains state"));
        drop(state);
        let mut lease = ReturnedJob {
            core: &self.core,
            index,
            job: Some(job),
        };
        // The owned closure protects the result even if diagnostic setup unwinds.
        // Its callback and lease destruction both occur inside the measured scope.
        Ok(Some(self.observe_consumption(trace, index, move || {
            consume(
                lease
                    .job
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("consumer lease owns state")),
            )
        })))
    }

    /// The callback owns its result lease before any diagnostic operation runs.
    fn observe_consumption<R>(
        &self,
        trace: solarity_profiling::TraceContext,
        index: usize,
        consume: impl FnOnce() -> R,
    ) -> R {
        // The callback and lease return are useful consumption, not worker waiting.
        trace.link("cpu.phase.consume_need");
        let _origin = trace.enter();
        let mut profile = if self.service.is_some() {
            solarity_profiling::profile!("cpu.load.consume")
        } else {
            solarity_profiling::profile!("cpu.frame.consume")
        };
        profile.trace_owner(index as u64 + 1, 0);
        consume()
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
