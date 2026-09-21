//! Owned service continuations yield without allocating another task or waiting.

use super::{CpuTaskPermit, publication::Publication};
use crate::CpuTaskStep;
use crate::pool::{
    CpuTask,
    dispatch::{ServiceStep, ServiceTurn, Work, WorkClass},
    task::TaskOutcome,
};
use std::{
    ops::ControlFlow,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Instant,
};

/// Domain captures and logical admission stay in the same box through every turn.
struct Steps<F, T> {
    operation: Option<F>,
    control: Arc<crate::pool::task::TaskControl>,
    publication: Publication<T>,
    trace: solarity_profiling::TraceContext,
    queued: Option<(u64, Instant)>,
    waiting: Option<(u64, Instant)>,
    dependency: Option<crate::CpuTaskDependency>,
}

/// Disabled instrumentation takes no clock reading; enabled intervals carry their epoch.
fn queue_time() -> Option<(u64, Instant)> {
    let epoch = solarity_profiling::generation();
    (epoch != 0).then(|| (epoch, Instant::now()))
}

impl<F, T> ServiceStep for Steps<F, T>
where
    F: FnMut(&crate::JobContext<'_>) -> CpuTaskStep<T> + Send,
    T: Send,
{
    fn step(&mut self, worker: crate::pool::WorkerLane) -> ServiceTurn {
        let _trace = self.trace.enter();
        if let Some((epoch, waiting)) = self.waiting.take() {
            // Includes dependency publication and the later service queue turn;
            // do not misattribute this suspended wall time as worker execution.
            static WAIT: solarity_profiling::Site =
                solarity_profiling::Site::new("cpu.job.dependency_to_resume", false);
            WAIT.cpu_duration(epoch, "", waiting.elapsed());
        }
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
        let dependency = self.dependency.take();
        // Own the closure inside the unwind boundary. Its captured state must
        // retire on this worker before publishing failure or returning capacity.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            // Releasing an edge may change another source's demand. It runs on
            // the consumer's worker outside scheduler locks and inside containment.
            drop(dependency);
            match operation(&self.control.context(self.trace, worker)) {
                CpuTaskStep::Continue => {
                    self.operation = Some(operation);
                    CpuTaskStep::Continue
                }
                CpuTaskStep::Wait(dependency) => {
                    self.operation = Some(operation);
                    CpuTaskStep::Wait(dependency)
                }
                CpuTaskStep::Complete(value) => {
                    drop(operation);
                    CpuTaskStep::Complete(value)
                }
            }
        }));
        if !worker.environment.install() {
            self.operation = None;
            self.publication.finish(TaskOutcome::EnvironmentFailed);
            return ServiceTurn::Finished;
        }
        match outcome {
            Ok(CpuTaskStep::Continue) => {
                self.queued = queue_time();
                ServiceTurn::Ready
            }
            Ok(CpuTaskStep::Wait(dependency)) => {
                self.waiting = queue_time();
                ServiceTurn::Waiting(dependency)
            }
            Ok(CpuTaskStep::Complete(value)) => {
                self.publication.finish(TaskOutcome::Completed(value));
                ServiceTurn::Finished
            }
            Err(_) => {
                self.publication.finish(TaskOutcome::Panicked);
                ServiceTurn::Finished
            }
        }
    }

    fn cancel(&self) {
        self.control.cancel();
    }

    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }

    fn retain_dependency(
        &mut self,
        dependency: crate::CpuTaskDependency,
    ) -> crate::completion::Binding {
        debug_assert!(self.dependency.is_none());
        let binding = dependency.subscription.binder();
        self.dependency = Some(dependency);
        binding
    }

    fn dependency_demand(&self) -> Option<crate::CpuServiceInterest> {
        self.dependency
            .as_ref()
            .and_then(|dependency| dependency.interest.clone())
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
        let mut operation = operation;
        self.submit_resumable_with_context(move |context| match operation(context) {
            ControlFlow::Continue(()) => CpuTaskStep::Continue,
            ControlFlow::Break(value) => CpuTaskStep::Complete(value),
        })
    }

    /// Submits an owned operation that can discover shared-resource dependencies.
    /// A suspended operation retains its logical admission and captures, but no
    /// worker, bulk allowance or ready-queue turn. Readiness, cancellation and
    /// shutdown resume it exactly once. The operation must handle cancellation
    /// at its next safe boundary and eventually complete; it must never wait for
    /// another worker or suspend while owning the producer of its own dependency.
    ///
    /// Reserve each `CpuTaskDependency` before returning `Wait`. The domain keeps
    /// its typed source handle and reads the result when resumed; source failure
    /// wakes the consumer just as success does, without erasing the source error.
    pub fn submit_resumable_with_context<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnMut(&crate::JobContext<'_>) -> CpuTaskStep<T> + Send + 'static,
        T: Send + 'static,
    {
        let Self {
            pool,
            lease,
            notifier,
            identity,
            execution,
            control,
        } = self;
        let (publication, receiver) = Publication::new(lease, notifier, Arc::clone(&control));
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
                    waiting: None,
                    dependency: None,
                }),
            ),
            WorkClass::Background,
        );
        CpuTask::new(receiver, control, trace, Arc::downgrade(pool), identity)
    }
}
