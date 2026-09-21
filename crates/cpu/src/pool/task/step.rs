//! Discovered dependencies suspend owned services without occupying a worker.

use crate::completion::Subscription;
use crate::{CpuError, ReadyToken};

/// One pre-reserved readiness edge, consumed when an owned service suspends.
/// The source payload and producer remain with the domain. Any terminal source
/// outcome resumes the operation so it can report the original typed error.
pub struct CpuTaskDependency {
    pub(in crate::pool) subscription: Subscription,
    pub(in crate::pool) interest: Option<crate::CpuServiceInterest>,
}

impl CpuTaskDependency {
    /// Reserves the edge before relinquishing domain state to the scheduler.
    ///
    /// # Errors
    /// Rejects a stale generation or exhausted subscriber capacity. The caller
    /// retains its operation and can return its ordinary domain admission error.
    pub fn new(readiness: &ReadyToken) -> Result<Self, CpuError> {
        Ok(Self {
            subscription: readiness.reserve()?,
            interest: None,
        })
    }

    /// Follows the suspended consumer's urgency without overriding other source
    /// consumers. The domain retains the typed result and its demand owner.
    #[must_use]
    pub fn with_demand(mut self, interest: crate::CpuServiceInterest) -> Self {
        self.interest = Some(interest);
        self
    }
}

/// The next boundary of one admitted, owned loading operation.
pub enum CpuTaskStep<T> {
    /// Return the same operation to its service FIFO after a bounded turn.
    Continue,
    /// Retain captures and admission, releasing the worker until readiness,
    /// cancellation or shutdown. The next call must inspect its owned source
    /// result or cooperative cancellation before continuing domain work.
    Wait(CpuTaskDependency),
    /// Retire captures on the worker and publish the single final result.
    Complete(T),
}
