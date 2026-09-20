//! A synchronous kernel sees admission identity and cooperative control only.

use crate::{CpuError, CpuScratch, CpuWorkerScratch, ScratchScope};
use std::sync::atomic::{AtomicBool, Ordering};

/// Identity within a registered batch or one service admission. It is
/// diagnostic provenance, not a cross-domain product or a result access token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobIdentity {
    epoch: u64,
    index: usize,
}

impl JobIdentity {
    /// Batch activation generation, or the service control's allocation identity.
    #[must_use]
    pub const fn epoch(self) -> u64 {
        self.epoch
    }
    /// Zero-based batch admission order; a single service operation uses zero.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
}

/// Borrowed for one synchronous operation. No runtime state, worker wait,
/// allocator growth, global RNG or dependency consumption is exposed here.
/// The context cannot authorize scratch access from another thread:
/// ```compile_fail
/// fn transfer(context: &solarity_cpu::JobContext<'_>) {
///     std::thread::scope(|scope| {
///         scope.spawn(move || context.identity());
///     });
/// }
/// ```
pub struct JobContext<'job> {
    identity: JobIdentity,
    cancelled: &'job AtomicBool,
    trace: solarity_profiling::TraceContext,
    worker: crate::pool::WorkerLane,
    // A lane loan is synchronous and cannot be transferred to a foreign thread.
    affinity: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl<'job> JobContext<'job> {
    /// Executor admission owns the cancellation cell for this whole invocation.
    pub(crate) fn new(
        epoch: u64,
        index: usize,
        cancelled: &'job AtomicBool,
        trace: solarity_profiling::TraceContext,
        worker: crate::pool::WorkerLane,
    ) -> Self {
        Self {
            identity: JobIdentity { epoch, index },
            cancelled,
            trace,
            worker,
            affinity: std::marker::PhantomData,
        }
    }

    /// Stable activation provenance; this does not depend on worker identity.
    #[must_use]
    pub const fn identity(&self) -> JobIdentity {
        self.identity
    }

    /// Observe withdrawal only at a domain-approved safe boundary. Required
    /// gameplay state must finish coherently; this never forcibly interrupts it.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Borrow preadmitted temporary storage. Values are cleared on normal return
    /// and unwind, while allocation ownership and its byte charge remain reusable.
    pub fn scratch<'scope, T>(
        &'scope self,
        scratch: &'scope mut CpuScratch<T>,
    ) -> ScratchScope<'scope, T> {
        scratch.scope()
    }

    /// Loans this physical worker's preadmitted typed storage. No scheduler or
    /// storage lock is held during the callback; temporary values cannot escape.
    /// `requested` declares the operation's necessary element capacity and records
    /// its per-lane demand peak. The callback may use only admitted storage.
    /// ```compile_fail
    /// fn escape<'a>(context: &'a solarity_cpu::JobContext<'a>,
    ///     scratch: &'a solarity_cpu::CpuWorkerScratch<u8>) -> &'a u8 {
    ///     context.with_worker_scratch(scratch, 1, |scope| {
    ///         &scope.writer()[0]
    ///     }).unwrap()
    /// }
    /// ```
    ///
    /// # Errors
    /// Rejects another executor's binding, demand beyond admitted capacity or a
    /// nested loan of the same lane. Internal transfer-state failure is reported.
    pub fn with_worker_scratch<T, R>(
        &self,
        scratch: &CpuWorkerScratch<T>,
        requested: usize,
        operation: impl for<'scope> FnOnce(&mut ScratchScope<'scope, T>) -> R,
    ) -> Result<R, CpuError> {
        scratch.scope(self.worker, requested, operation)
    }

    /// Inherits the admitted trace rather than capturing a worker's unrelated
    /// current task. Disabled diagnostics take no timestamp or lock here.
    pub fn diagnostic_value(&self, name: &'static str, value: u64) {
        if self.trace.is_sampled() {
            self.trace.value(
                name,
                self.identity.index as u64 + 1,
                self.identity.epoch,
                value,
            );
        }
    }
}
