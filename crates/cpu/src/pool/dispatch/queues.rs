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
            WorkClass::Frame if work.urgent() => queues.urgent.push(work),
            WorkClass::Frame => queues.frame.push(work),
            WorkClass::Background => queues.service(work.service()).push_back(work),
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
        let Queues { frame, urgent, .. } = &mut *queues;
        frame.promote(owner, urgent);
        self.publish_queued(&queues);
        drop(queues);
        self.ready.notify_all();
    }

    /// Publishes a cheap boundary-yield hint while queue contents remain locked.
    pub(super) fn publish_queued(&self, queues: &Queues) {
        // Two bits per cost level, then metadata/service predicates. Keeping the
        // complete hint in one atomic avoids mixing snapshots across priorities.
        let bits = queues.frame.level()
            | (queues.urgent.level() << 2)
            | (u8::from(!queues.priority.is_empty()) << 4)
            | (u8::from(
                queues.active_service < self.bulk_limit
                    && (!queues.required.is_empty() || !queues.retirement.is_empty()),
            ) << 5);
        self.queued.store(bits, Ordering::Release);
    }

    /// A kernel boundary yields for metadata, urgent prerequisites, required
    /// flexible service, or an equal/heavier ready runner in the same priority.
    /// Equal costs receive FIFO turns; lower-cost phases cannot interrupt a
    /// heavier phase. Queue predicates are rechecked on dispatch.
    pub(crate) fn should_yield(&self, urgent: bool, flexible: bool, cost: u8) -> bool {
        let queued = self.queued.load(Ordering::Acquire);
        let urgent_level = (queued >> 2) & 3;
        let level = if urgent { urgent_level } else { queued & 3 };
        // Nonempty levels encode bin + 1, so this includes equal-cost work.
        queued & 16 != 0
            || (!urgent && urgent_level != 0)
            || (flexible && queued & 32 != 0)
            || level > cost
    }

    /// A selected runner may lose its heavy nodes to another worker before it
    /// claims state. Only strictly heavier work can preempt before a first kernel;
    /// equal-cost runners must execute once to preserve FIFO progress.
    pub(crate) fn heavier_is_queued(&self, urgent: bool, cost: u8) -> bool {
        let queued = self.queued.load(Ordering::Acquire);
        let urgent_level = (queued >> 2) & 3;
        let level = if urgent { urgent_level } else { queued & 3 };
        (!urgent && urgent_level != 0) || level > cost + 1
    }

    /// Queue-only adjustment follows phase metadata publication. No phase lock
    /// is held here, matching urgency promotion's lock order.
    pub(crate) fn reclassify_cost(&self, owner: &Arc<dyn ReadyWork>) {
        // Even an apparently empty queue must synchronize: a concurrent push
        // may have classified this owner before publishing its queue hint.
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|_| unreachable!("queue metadata cannot panic"));
        queues.frame.reclassify(owner);
        queues.urgent.reclassify(owner);
        self.publish_queued(&queues);
        drop(queues);
        self.ready.notify_all();
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
    pub(super) fn service(
        &mut self,
        service: CpuService,
    ) -> &mut crate::storage::StorageDeque<Work> {
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
        let previous = CpuService::from_raw(previous);
        let source = queues.service(previous);
        let mut moved = None;
        for _ in 0..source.len() {
            let work = source
                .pop_front()
                .unwrap_or_else(|| unreachable!("queue scan retains its length"));
            if work
                .service_identity()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, identity))
            {
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
