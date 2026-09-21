//! Indivisible service calls retain the same admission and completion protocol.

use super::{CpuTaskPermit, publication::Publication};
use crate::pool::{
    CpuTask,
    dispatch::{Work, WorkClass},
    task::TaskOutcome,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Instant,
};

impl CpuTaskPermit<'_> {
    /// Transfers the admitted operation to the executor's FIFO queue.
    /// Admission cannot fail after the caller relinquishes its inputs.
    pub fn submit<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.submit_with_context(move |_| operation())
    }

    /// Supplies admitted control to an indivisible operation. Cancellation cannot
    /// preempt a foreign call; the operation chooses safe checks around it.
    pub fn submit_with_context<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnOnce(&crate::JobContext<'_>) -> T + Send + 'static,
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
        let (mut publication, receiver) = Publication::new(lease, notifier, Arc::clone(&control));
        let executing = Arc::clone(&control);
        let epoch = solarity_profiling::generation();
        let queued = (epoch != 0).then(Instant::now);
        let trace = solarity_profiling::TraceContext::capture().fork("cpu.job");
        pool.push(
            Work::Once(
                Arc::clone(&identity),
                execution,
                Box::new(move |worker| {
                    let _trace = trace.enter();
                    let _profile = solarity_profiling::profile!("cpu.job.execute");
                    if let Some(queued) = queued {
                        static QUEUE: solarity_profiling::Site =
                            solarity_profiling::Site::new("cpu.job.queue_wait", false);
                        QUEUE.cpu_duration(epoch, "", queued.elapsed());
                    }
                    let mut outcome = match catch_unwind(AssertUnwindSafe(|| {
                        operation(&executing.context(trace, worker))
                    })) {
                        Ok(value) => TaskOutcome::Completed(value),
                        Err(_panic_payload) => TaskOutcome::Panicked,
                    };
                    // Foreign calls and captured destructors may alter thread
                    // controls. Restore before any consumer can observe success.
                    if !worker.environment.install() {
                        outcome = TaskOutcome::EnvironmentFailed;
                    }
                    publication.finish(outcome);
                }),
            ),
            WorkClass::Background,
        );
        CpuTask::new(receiver, control, trace, Arc::downgrade(pool), identity)
    }
}
