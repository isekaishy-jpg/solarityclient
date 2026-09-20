//! Owned service continuations yield without allocating another task or waiting.

use super::{CpuTaskPermit, publication::Publication};
use crate::pool::{
    CpuTask,
    dispatch::{ServiceStep, Work, WorkClass},
    task::TaskOutcome,
};
use std::{
    ops::ControlFlow,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, atomic::AtomicU8},
    time::Instant,
};

/// Domain captures and logical admission stay in the same box through every turn.
struct Steps<F, T> {
    operation: Option<F>,
    control: Arc<crate::pool::task::TaskControl>,
    publication: Publication<T>,
    trace: solarity_profiling::TraceContext,
    queued: Option<(u64, Instant)>,
}

/// Disabled instrumentation takes no clock reading; enabled intervals carry their epoch.
fn queue_time() -> Option<(u64, Instant)> {
    let epoch = solarity_profiling::generation();
    (epoch != 0).then(|| (epoch, Instant::now()))
}

impl<F, T> ServiceStep for Steps<F, T>
where
    F: FnMut(&crate::JobContext<'_>) -> ControlFlow<T> + Send,
    T: Send,
{
    fn step(&mut self, worker: crate::pool::WorkerLane) -> bool {
        let _trace = self.trace.enter();
        let _profile = solarity_profiling::profile!("cpu.job.execute");
        if let Some((epoch, queued)) = self.queued.take() {
            static QUEUE: solarity_profiling::Site =
                solarity_profiling::Site::new("cpu.job.queue_wait", false);
            QUEUE.cpu_duration(epoch, "", queued.elapsed());
        }
        let mut operation = self
            .operation
            .take()
            .unwrap_or_else(|| unreachable!("only a pending operation is resumed"));
        // Own the closure inside the unwind boundary. Its captured state must
        // retire on this worker before publishing failure or returning capacity.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            match operation(&self.control.context(self.trace, worker)) {
                ControlFlow::Continue(()) => {
                    self.operation = Some(operation);
                    ControlFlow::Continue(())
                }
                ControlFlow::Break(value) => {
                    drop(operation);
                    ControlFlow::Break(value)
                }
            }
        }));
        if !worker.environment.install() {
            self.operation = None;
            self.publication.finish(TaskOutcome::EnvironmentFailed);
            return false;
        }
        match outcome {
            Ok(ControlFlow::Continue(())) => {
                self.queued = queue_time();
                true
            }
            Ok(ControlFlow::Break(value)) => {
                self.publication.finish(TaskOutcome::Completed(value));
                false
            }
            Err(_) => {
                self.publication.finish(TaskOutcome::Panicked);
                false
            }
        }
    }
}

impl CpuTaskPermit<'_> {
    /// Submits a finite operation whose owned state resumes at explicit boundaries.
    ///
    /// Each call must perform a bounded amount of work, never wait on another
    /// worker, and eventually return `ControlFlow::Break(result)`. `Continue(())`
    /// returns the same operation to its current service FIFO. The task slot,
    /// captures and result channel remain owned until terminal publication,
    /// including during shutdown or after the consumer drops its handle.
    /// No step creates a thread, task allocation, or new admission reservation.
    /// A single indivisible domain call still cannot be preempted.
    pub fn submit_steps<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnMut() -> ControlFlow<T> + Send + 'static,
        T: Send + 'static,
    {
        let mut operation = operation;
        self.submit_steps_with_context(move |_| operation())
    }

    /// Resumes the same admitted identity with scoped scratch, diagnostics and
    /// cooperative withdrawal. Required cleanup must ignore withdrawal and drain;
    /// no cancellation check may discard half-advanced gameplay or borrowed state.
    pub fn submit_steps_with_context<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnMut(&crate::JobContext<'_>) -> ControlFlow<T> + Send + 'static,
        T: Send + 'static,
    {
        let Self {
            pool,
            lease,
            notifier,
            service,
            execution,
            control,
        } = self;
        let (publication, receiver) = Publication::new(lease, notifier, Arc::clone(&control));
        let identity = Arc::new(AtomicU8::new(service as u8));
        let trace = solarity_profiling::TraceContext::capture().fork("cpu.job");
        pool.push(
            Work::Sliced(
                Arc::clone(&identity),
                execution,
                Box::new(Steps {
                    operation: Some(operation),
                    control: Arc::clone(&control),
                    publication,
                    trace,
                    queued: queue_time(),
                }),
            ),
            WorkClass::Background,
        );
        CpuTask::new(receiver, control, trace, Arc::downgrade(pool), identity)
    }
}
