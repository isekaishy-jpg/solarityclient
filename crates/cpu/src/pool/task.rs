//! Typed CPU task completion boundary.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

use crate::pool::CpuError;

/// Internal worker outcome transported without exposing panic payloads.
pub(crate) enum TaskOutcome<T> {
    /// The task returned normally.
    Completed(T),
    /// The task unwound through the executor boundary.
    Panicked,
}

/// The single-owner completion handle for one admitted CPU task.
///
/// Dropping this handle discards the result but does not detach executor
/// ownership: shutdown still waits for the underlying task to finish.
pub struct CpuTask<T> {
    receiver: Receiver<TaskOutcome<T>>,
    finished: Arc<AtomicBool>,
}

impl<T> CpuTask<T> {
    /// Creates a completion handle for the executor's one-result channel.
    pub(crate) fn new(receiver: Receiver<TaskOutcome<T>>, finished: Arc<AtomicBool>) -> Self {
        Self { receiver, finished }
    }

    /// Waits for the task and transfers ownership of its result.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::TaskPanicked`] when the task unwound or
    /// [`CpuError::CompletionLost`] if executor invariants were violated.
    pub fn join(self) -> Result<T, CpuError> {
        let outcome = self
            .receiver
            .recv()
            .map_err(|_disconnected| CpuError::CompletionLost)?;
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
