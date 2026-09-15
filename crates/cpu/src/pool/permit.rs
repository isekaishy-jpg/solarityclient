//! Reserved cold-operation submission and owned result publication.

use super::dispatch::{Dispatch, Work, WorkClass};
use super::task::TaskOutcome;
use super::worker::WorkerLease;
use super::{CpuService, CpuTask};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::sync_channel,
};
use std::time::Instant;

/// One reserved CPU task slot, borrowing the executor until submission.
/// Its lease also counts toward the running-plus-queued capacity bound.
pub struct CpuTaskPermit<'executor> {
    pool: &'executor Arc<Dispatch>,
    service: CpuService,
    lease: WorkerLease,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

impl<'executor> CpuTaskPermit<'executor> {
    /// Binds admission to its service bucket before caller input ownership moves.
    pub(super) fn new(
        pool: &'executor Arc<Dispatch>,
        lease: WorkerLease,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
        service: CpuService,
    ) -> Self {
        Self {
            pool,
            lease,
            notifier,
            service,
        }
    }

    /// Transfers the admitted operation to the executor's FIFO queue.
    /// Admission cannot fail after the caller relinquishes its inputs.
    pub fn submit<F, T>(self, operation: F) -> CpuTask<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let Self {
            pool,
            lease,
            notifier,
            service,
        } = self;
        let (sender, receiver) = sync_channel(1);
        let finished = Arc::new(AtomicBool::new(false));
        let finished_by_worker = Arc::clone(&finished);
        let epoch = solarity_profiling::generation();
        let queued = (epoch != 0).then(Instant::now);
        let trace = solarity_profiling::TraceContext::capture().fork("cpu.job");
        let identity = Arc::new(AtomicU8::new(service as u8));
        pool.push(
            Work::Once(
                Arc::clone(&identity),
                Box::new(move || {
                    let _trace = trace.enter();
                    let _profile = solarity_profiling::profile!("cpu.job.execute");
                    if let Some(queued) = queued {
                        static QUEUE: solarity_profiling::Site =
                            solarity_profiling::Site::new("cpu.job.queue_wait", false);
                        QUEUE.cpu_duration(epoch, "", queued.elapsed());
                    }
                    let outcome = match catch_unwind(AssertUnwindSafe(operation)) {
                        Ok(value) => TaskOutcome::Completed(value),
                        Err(_panic_payload) => TaskOutcome::Panicked,
                    };
                    // Publish completion only after returning admission capacity.
                    drop(lease);
                    let _completion_observed = sender.send(outcome);
                    finished_by_worker.store(true, Ordering::Release);
                    if let Some(notifier) = notifier {
                        notifier.notify();
                    }
                }),
            ),
            WorkClass::Background(service),
        );
        CpuTask::new(receiver, finished, trace, Arc::downgrade(pool), identity)
    }
}
