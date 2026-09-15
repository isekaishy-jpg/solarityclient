//! One terminal result releases logical admission before waking its consumer.

use crate::pool::{task::TaskOutcome, worker::WorkerLease};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

/// Shared publication ordering for indivisible and resumable operations.
pub(super) struct Publication<T> {
    lease: Option<WorkerLease>,
    sender: SyncSender<TaskOutcome<T>>,
    finished: Arc<AtomicBool>,
    notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
}

impl<T> Publication<T> {
    /// Creates one result channel for the entire operation, never one per step.
    pub(super) fn new(
        lease: WorkerLease,
        notifier: Option<Arc<dyn crate::CoordinatorNotifier>>,
    ) -> (Self, Receiver<TaskOutcome<T>>, Arc<AtomicBool>) {
        let (sender, receiver) = sync_channel(1);
        let finished = Arc::new(AtomicBool::new(false));
        (
            Self {
                lease: Some(lease),
                sender,
                finished: Arc::clone(&finished),
                notifier,
            },
            receiver,
            finished,
        )
    }

    /// Domain captures have already retired; publish exactly one terminal outcome.
    pub(super) fn finish(&mut self, outcome: TaskOutcome<T>) {
        drop(self.lease.take());
        let _observed = self.sender.send(outcome);
        self.finished.store(true, Ordering::Release);
        if let Some(notifier) = &self.notifier {
            notifier.notify();
        }
    }
}
