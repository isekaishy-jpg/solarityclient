//! Bounded ready buckets, promotion and kernel-boundary service hints.

use super::{CpuService, Dispatch, Queues, ReadyWork, Work, WorkClass};
use std::sync::{Arc, atomic::Ordering};

impl Dispatch {
    /// Captures acquisition time, publishing it only after the guard unlocks.
    pub(super) fn lock(&self) -> crate::pool::observation::ObservedGuard<'_, Queues> {
        static WAIT: solarity_profiling::Site =
            solarity_profiling::Site::new("cpu.dispatch.lock_wait", true);
        crate::pool::observation::ObservedGuard::lock(&self.queues, &WAIT)
    }

    /// Notification precedes observer emission so cold registry work cannot
    /// delay the wake it is measuring. The durable predicate is already set.
    pub(super) fn notify_ready(
        &self,
        mut queues: crate::pool::observation::ObservedGuard<'_, Queues>,
    ) {
        queues.observe_notification();
        let acquisition = queues.unlock();
        self.ready.notify_all();
        if let Some(acquisition) = acquisition {
            acquisition.report();
        }
    }

    /// Publishes one cold service or priority record using the same queue boundary.
    pub(crate) fn push(&self, work: Work, class: WorkClass) {
        let mut queues = self.lock();
        let queued = crate::pool::observation::SampleTime::now();
        queues.insert(super::QueuedWork::new(work, queued), class);
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }

    /// All reserved runners become visible under one lock and one wake. Creating
    /// records only clones admitted owners; no domain operation runs here.
    pub(crate) fn push_runners(
        &self,
        owner: Arc<dyn ReadyWork>,
        service: Option<Arc<std::sync::atomic::AtomicU8>>,
        count: usize,
    ) {
        if count == 0 {
            return;
        }
        let mut queues = self.lock();
        let queued = crate::pool::observation::SampleTime::now();
        for _ in 0..count {
            let (work, class) = match &service {
                Some(service) => (
                    Work::Loading(Arc::clone(service), Arc::clone(&owner)),
                    WorkClass::Background,
                ),
                None => (Work::Retained(Arc::clone(&owner)), WorkClass::Frame),
            };
            queues.insert(super::QueuedWork::new(work, queued), class);
        }
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }

    /// Moves this phase's already queued runners, retaining FIFO ties. The scan
    /// is over admitted runner records only, once per phase promotion.
    pub(crate) fn promote(&self, owner: &Arc<dyn ReadyWork>) {
        let mut queues = self.lock();
        let Queues { frame, urgent, .. } = &mut *queues;
        frame.promote(owner, urgent);
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }

    /// Publishes a cheap boundary-yield hint while queue contents remain locked.
    pub(super) fn publish_queued(&self, queues: &Queues) {
        // Two bits per cost level, then metadata/service predicates. Keeping the
        // complete hint in one atomic avoids mixing snapshots across priorities.
        let bits = queues.frame.level()
            | (queues.urgent.level() << 2)
            | (u8::from(!queues.priority.is_empty()) << 4)
            | (u8::from(
                queues
                    .required
                    .eligible(queues.active_bulk < self.bulk_limit)
                    || queues
                        .retirement
                        .eligible(queues.active_bulk < self.bulk_limit),
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
        let mut queues = self.lock();
        queues.frame.reclassify(owner);
        queues.urgent.reclassify(owner);
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }

    /// Closes the durable sleep predicate after the admission owners drain.
    pub(crate) fn stop(&self) {
        self.lock().stopping = true;
        self.ready.notify_all();
    }
}

impl Queues {
    /// No per-worker scan or timestamp is taken outside sampled detail frames.
    pub(super) fn observe_notification(&mut self) {
        if let Some(now) = crate::pool::observation::SampleTime::now() {
            for sleeper in self.sleepers.iter_mut() {
                sleeper.notify(now);
            }
        }
    }

    /// Classification reads atomics only and retains original FIFO ties.
    fn insert(&mut self, work: super::QueuedWork, class: WorkClass) {
        match class {
            WorkClass::Frame if work.urgent() => self.urgent.push(work),
            WorkClass::Frame => self.frame.push(work),
            WorkClass::Background => self.service(work.service()).push_back(work),
            WorkClass::Priority => self.priority.push_back(work),
        }
    }

    /// Selects one reserved FIFO without allocating or invoking domain code.
    pub(super) fn service(&mut self, service: CpuService) -> &mut super::service::ServiceQueue {
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
        let mut queues = self.lock();
        let previous = identity.swap(service as u8, Ordering::AcqRel);
        if previous == service as u8 {
            return;
        }
        let previous = CpuService::from_raw(previous);
        // A loading phase publishes several runners with the same interest.
        // Move every matching record; retaining only the last one loses runnable
        // ownership and can strand a phase after its dependency is promoted.
        let count = queues.service(previous).len();
        for _ in 0..count {
            let work = queues
                .service(previous)
                .pop_front()
                .unwrap_or_else(|| unreachable!("queue scan retains its length"));
            if work
                .service_identity()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, identity))
            {
                queues.service(service).push_back(work);
            } else {
                queues.service(previous).push_back(work);
            }
        }
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }
}
