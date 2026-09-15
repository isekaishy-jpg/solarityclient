//! Typed CPU task completion boundary.

use super::{CpuService, dispatch::Dispatch};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};

use crate::pool::CpuError;

/// Internal worker outcome transported without exposing panic payloads.
pub(crate) enum TaskOutcome<T> {
    /// The task returned normally.
    Completed(T),
    /// The task unwound through the executor boundary.
    Panicked,
}

/// Scheduling control can be shared without sharing consumption of the task result.
#[derive(Clone)]
pub struct CpuServiceControl {
    dispatch: Weak<Dispatch>,
    service: Arc<AtomicU8>,
}

impl CpuServiceControl {
    /// Changes queued service metadata; no domain callback or worker wait is involved.
    pub fn set_service(&self, service: CpuService) {
        if self.service.load(Ordering::Acquire) != service as u8
            && let Some(dispatch) = self.dispatch.upgrade()
        {
            dispatch.reclassify(&self.service, service);
        }
    }

    /// Reports the current scheduling class of this task identity.
    #[must_use]
    pub fn service(&self) -> CpuService {
        CpuService::from_raw(self.service.load(Ordering::Acquire))
    }
}

/// The single-owner completion handle for one admitted CPU task.
///
/// Dropping this handle discards the result but does not detach executor
/// ownership: shutdown still waits for the underlying task to finish.
pub struct CpuTask<T> {
    receiver: Receiver<TaskOutcome<T>>,
    finished: Arc<AtomicBool>,
    trace: solarity_profiling::TraceContext,
    dispatch: Weak<Dispatch>,
    service: Arc<AtomicU8>,
}

impl<T> CpuTask<T> {
    /// Creates a completion handle for the executor's one-result channel.
    pub(crate) fn new(
        receiver: Receiver<TaskOutcome<T>>,
        finished: Arc<AtomicBool>,
        trace: solarity_profiling::TraceContext,
        dispatch: Weak<Dispatch>,
        service: Arc<AtomicU8>,
    ) -> Self {
        Self {
            receiver,
            finished,
            trace,
            dispatch,
            service,
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
        self.finished.load(Ordering::Acquire)
    }
}

/// Converts the private worker representation to the public stable error.
fn decode_outcome<T>(outcome: TaskOutcome<T>) -> Result<T, CpuError> {
    match outcome {
        TaskOutcome::Completed(value) => Ok(value),
        TaskOutcome::Panicked => Err(CpuError::TaskPanicked),
    }
}
