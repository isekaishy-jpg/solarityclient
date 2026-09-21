//! Typed CPU task completion boundary.

use super::{CpuServiceControl, TaskControl, TaskInterest};
use crate::pool::{CpuService, dispatch::Dispatch};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};

use crate::pool::CpuError;

/// Internal worker outcome transported without exposing panic payloads.
pub(crate) enum TaskOutcome<T> {
    /// The task returned normally.
    Completed(T),
    /// The task unwound through the executor boundary.
    Panicked,
    /// The worker could not reestablish the admitted numeric execution contract.
    EnvironmentFailed,
}

/// The single-owner completion handle for one admitted CPU task.
///
/// Dropping this handle requests cooperative withdrawal and discards the result,
/// but does not detach executor ownership: shutdown still waits for the task.
/// Contextual operations choose their safe stopping boundaries; legacy operations
/// and mandatory cleanup continue to completion.
pub struct CpuTask<T> {
    receiver: Receiver<TaskOutcome<T>>,
    control: TaskInterest,
    trace: solarity_profiling::TraceContext,
    dispatch: Weak<Dispatch>,
    service: Arc<AtomicU8>,
}

impl<T> CpuTask<T> {
    /// Creates a completion handle for the executor's one-result channel.
    pub(in crate::pool) fn new(
        receiver: Receiver<TaskOutcome<T>>,
        control: Arc<TaskControl>,
        trace: solarity_profiling::TraceContext,
        dispatch: Weak<Dispatch>,
        service: Arc<AtomicU8>,
    ) -> Self {
        Self {
            receiver,
            control: TaskInterest(control),
            trace,
            dispatch,
            service,
        }
    }

    /// Requests cooperative withdrawal. Contextual operations observe this at
    /// their own safe boundaries; indivisible calls and required cleanup still
    /// finish. The handle retains its result and can be joined normally.
    pub fn cancel(&self) {
        self.control.0.cancel();
        if let Some(dispatch) = self.dispatch.upgrade() {
            dispatch.resume_cancelled(&self.service);
        }
    }

    /// Changes queued demand without repeating work or touching domain state.
    /// De-escalation is explicit when a selected consumer no longer needs a
    /// prewarm. A running operation remains indivisible and retains ownership.
    pub fn set_service(&self, service: CpuService) {
        if self.service.load(Ordering::Acquire) != service as u8
            && let Some(dispatch) = self.dispatch.upgrade()
        {
            dispatch.reclassify(&self.service, service);
        }
    }

    /// Exposes scheduling metadata without transferring or cloning result ownership.
    #[must_use]
    pub fn service_control(&self) -> CpuServiceControl {
        CpuServiceControl {
            dispatch: self.dispatch.clone(),
            service: Arc::clone(&self.service),
        }
    }

    /// Waits for the task and transfers ownership of its result.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::TaskPanicked`] when the task unwound or
    /// [`CpuError::CompletionLost`] if executor invariants were violated.
    pub fn join(self) -> Result<T, CpuError> {
        if crate::environment::is_worker() && !self.is_finished() {
            return Err(CpuError::WorkerWait);
        }
        self.set_service(CpuService::Required);
        let _profile_scope = solarity_profiling::profile!("cpu.pool.task.join");
        self.trace.link("cpu.job.join");
        let outcome = self
            .receiver
            .recv()
            .map_err(|_disconnected| CpuError::CompletionLost)?;
        self.trace.link("cpu.job.consume");
        decode_outcome(outcome)
    }

    /// Reports whether joining can complete without waiting for worker code.
    ///
    /// This query does not consume the result and is suitable for an
    /// interactive owner polling a finite set of outstanding tasks.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.control.0.finished.load(Ordering::Acquire)
    }
}

impl<T> Drop for CpuTask<T> {
    fn drop(&mut self) {
        if !self.is_finished() {
            self.cancel();
        }
    }
}

/// Converts the private worker representation to the public stable error.
fn decode_outcome<T>(outcome: TaskOutcome<T>) -> Result<T, CpuError> {
    match outcome {
        TaskOutcome::Completed(value) => Ok(value),
        TaskOutcome::Panicked => Err(CpuError::TaskPanicked),
        TaskOutcome::EnvironmentFailed => Err(CpuError::WorkerEnvironment),
    }
}
