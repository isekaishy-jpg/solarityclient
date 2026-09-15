//! Bounded ready buckets, promotion and kernel-boundary service hints.

use super::{Dispatch, Queues, ReadyWork, Work, WorkClass};
use std::sync::{Arc, atomic::Ordering};

impl Dispatch {
    /// Enqueues already admitted work before waking eligible sleepers.
    pub(crate) fn push(&self, work: Work, class: WorkClass) {
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"));
        match class {
            WorkClass::Frame if work.urgent() => queues.urgent.push_back(work),
            WorkClass::Frame => queues.frame.push_back(work),
            WorkClass::Background => queues.background.push_back(work),
            WorkClass::Priority => queues.priority.push_back(work),
        }
        self.publish_queued(&queues);
        drop(queues);
        // A single notify could wake a protected worker for background work.
        self.ready.notify_all();
    }

    /// Moves this phase's already queued runners, retaining FIFO ties. The scan
    /// is over admitted runner records only, once per phase promotion.
    pub(crate) fn promote(&self, owner: &Arc<dyn ReadyWork>) {
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|_| unreachable!("queue metadata cannot panic"));
        let count = queues.frame.len();
        for _ in 0..count {
            let work = queues
                .frame
                .pop_front()
                .unwrap_or_else(|| unreachable!("queue scan retains its length"));
            if matches!(&work, Work::Retained(candidate) if Arc::ptr_eq(candidate, owner)) {
                queues.urgent.push_back(work);
            } else {
                queues.frame.push_back(work);
            }
        }
        self.publish_queued(&queues);
        drop(queues);
        self.ready.notify_all();
    }

    /// Publishes a cheap boundary-yield hint while queue contents remain locked.
    pub(super) fn publish_queued(&self, queues: &Queues) {
        let bits = u8::from(!queues.urgent.is_empty())
            | (u8::from(!queues.priority.is_empty()) << 1)
            | (u8::from(!queues.background.is_empty()) << 2);
        self.queued.store(bits, Ordering::Release);
    }

    /// A kernel boundary yields for metadata, urgent prerequisites, or required
    /// service on the flexible lane. Queue predicates are rechecked on dispatch.
    pub(crate) fn should_yield(&self, urgent: bool, flexible: bool) -> bool {
        let queued = self.queued.load(Ordering::Acquire);
        queued & 2 != 0 || (!urgent && queued & 1 != 0) || (flexible && queued & 4 != 0)
    }

    /// Closes the durable sleep predicate after the admission owners drain.
    pub(crate) fn stop(&self) {
        self.queues
            .lock()
            .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"))
            .stopping = true;
        self.ready.notify_all();
    }
}
