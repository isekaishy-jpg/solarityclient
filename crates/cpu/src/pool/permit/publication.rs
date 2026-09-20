//! One terminal result releases logical admission before waking its consumer.

use crate::pool::{
    task::{TaskControl, TaskOutcome},
    worker::WorkerLease,
};
use std::sync::{
    Arc,
    atomic::Ordering,
    mpsc::{Receiver, SyncSender, sync_channel},
};

/// Shared publication ordering for indivisible and resumable operations.
pub(super) struct Publication<T> {
    lease: Option<WorkerLease>,
    sender: SyncSender<TaskOutcome<T>>,
    control: Arc<TaskControl>,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

impl<T> Publication<T> {
    /// Creates one result channel for the entire operation, never one per step.
    pub(super) fn new(
        lease: WorkerLease,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
        control: Arc<TaskControl>,
    ) -> (Self, Receiver<TaskOutcome<T>>) {
        let (sender, receiver) = sync_channel(1);
        (
            Self {
                lease: Some(lease),
                sender,
                control,
                notifier,
            },
            receiver,
        )
    }

    /// Domain captures have already retired; publish exactly one terminal outcome.
    pub(super) fn finish(&mut self, outcome: TaskOutcome<T>) {
        drop(self.lease.take());
        let observed = self.sender.send(outcome);
        self.control.finished.store(true, Ordering::Release);
        if let Some(notifier) = &self.notifier {
            notifier.notify();
        }
        // Withdrawal retains worker-side destruction. An arbitrary result Drop
        // must not kill the persistent worker or strand the dispatch bulk count.
        if let Err(unobserved) = observed {
            let dropped =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(unobserved)));
            if dropped.is_err() {
                solarity_profiling::profile_value!("cpu.job.abandoned_result_drop_panic", 1);
            }
        }
    }
}
