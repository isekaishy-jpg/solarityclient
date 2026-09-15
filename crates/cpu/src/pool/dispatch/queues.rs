//! Bounded ready buckets, promotion and kernel-boundary service hints.

use super::{CpuService, Dispatch, Queues, ReadyWork, Work, WorkClass};
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
            WorkClass::Background(service) => queues.service(service).push_back(work),
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
            | (u8::from(!queues.required.is_empty() || !queues.retirement.is_empty()) << 2);
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

impl Queues {
    /// Selects one reserved FIFO without allocating or invoking domain code.
    fn service(&mut self, service: CpuService) -> &mut crate::storage::StorageDeque<Work> {
        match service {
            CpuService::Required => &mut self.required,
            CpuService::Retirement => &mut self.retirement,
            CpuService::Speculative => &mut self.speculative,
        }
    }
}

impl Dispatch {
    /// Updates an admitted request at a demand-change boundary. A running call
    /// is indivisible; only a still-queued operation moves between FIFOs.
    pub(crate) fn reclassify(
        &self,
        identity: &Arc<std::sync::atomic::AtomicU8>,
        service: CpuService,
    ) {
        if identity.load(Ordering::Acquire) == service as u8 {
            return;
        }
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|_| unreachable!("queue metadata cannot panic"));
        let previous = identity.swap(service as u8, Ordering::AcqRel);
        if previous == service as u8 {
            return;
        }
        let previous = match previous {
            0 => CpuService::Required,
            1 => CpuService::Retirement,
            2 => CpuService::Speculative,
            _ => unreachable!("service identity contains a CpuService discriminant"),
        };
        let source = queues.service(previous);
        let mut moved = None;
        for _ in 0..source.len() {
            let work = source
                .pop_front()
                .unwrap_or_else(|| unreachable!("queue scan retains its length"));
            if matches!(&work, Work::Once(candidate, _) if Arc::ptr_eq(candidate, identity)) {
                moved = Some(work);
            } else {
                source.push_back(work);
            }
        }
        if let Some(work) = moved {
            queues.service(service).push_back(work);
        }
        self.publish_queued(&queues);
        drop(queues);
        self.ready.notify_all();
    }
}
