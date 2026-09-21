//! Durable dependency gates retain service ownership in pre-admitted pool storage.

use super::{Dispatch, QueuedWork, Queues, Work};
use crate::completion::{Binding, ReadySink};
use crate::{CpuTaskDependency, JobOutcome};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// Slot serials reject a delayed delivery after cancellation and subsequent reuse.
#[derive(Default)]
pub(super) struct ParkedService {
    serial: u64,
    work: Option<Work>,
}

impl ParkedService {
    /// Only shared scheduling metadata escapes the dispatch lock.
    pub(super) fn demand(&self) -> Option<(Arc<AtomicU8>, crate::CpuServiceInterest)> {
        match &self.work {
            Some(Work::Sliced(identity, _, operation)) => operation
                .dependency_demand()
                .map(|interest| (Arc::clone(identity), interest)),
            _ => None,
        }
    }
}

/// Refresh outside every queue lock. Recheck after delivery so a concurrent
/// demotion/promotion cannot leave the dependency at an older scheduling class.
pub(super) fn follow_demand(identity: &Arc<AtomicU8>, interest: &crate::CpuServiceInterest) {
    loop {
        let current = identity.load(Ordering::Acquire);
        interest.set_service(crate::CpuService::from_raw(current));
        if identity.load(Ordering::Acquire) == current {
            return;
        }
    }
}

impl Queues {
    /// Runs under the same lock as cancellation wake and shutdown. No domain
    /// callback or domain destruction occurs here; the operation stays owned.
    pub(super) fn park(
        &mut self,
        mut work: Work,
        dependency: CpuTaskDependency,
    ) -> Option<(Binding, u64, usize)> {
        let Work::Sliced(_, _, operation) = &mut work else {
            unreachable!("only resumable services discover dependencies")
        };
        let binding = operation.retain_dependency(dependency);
        if self.suspension_closed {
            operation.cancel();
        }
        if operation.is_cancelled() {
            self.service(work.service()).push_back(QueuedWork::new(
                work,
                crate::pool::observation::SampleTime::now(),
            ));
            return None;
        }
        let (index, slot) = self
            .parked
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.work.is_none())
            .unwrap_or_else(|| unreachable!("logical admission bounds parked services"));
        slot.serial = slot.serial.checked_add(1).unwrap_or_else(|| {
            unreachable!("a service slot cannot cycle 2^64 times in a process lifetime")
        });
        slot.work = Some(work);
        Some((binding, slot.serial, index))
    }

    /// Returning ready preserves current demand; no task allocation or CPU
    /// readmission is needed, including when every logical slot is occupied.
    fn resume(&mut self, index: usize) {
        let slot = &mut self.parked[index];
        let Some(work) = slot.work.take() else { return };
        self.service(work.service()).push_back(QueuedWork::new(
            work,
            crate::pool::observation::SampleTime::now(),
        ));
    }

    /// Shutdown invokes each domain's contextual cleanup on a worker. A pending
    /// external producer cannot keep the pool alive indefinitely.
    pub(super) fn resume_shutdown(&mut self) {
        for index in 0..self.parked.len() {
            if let Some(Work::Sliced(_, _, operation)) = &self.parked[index].work {
                operation.cancel();
            }
            self.resume(index);
        }
    }
}

impl Dispatch {
    /// Connect a newly parked edge after releasing the queue lock. Any concurrent
    /// wake/removal simply leaves no pending producer to promote.
    pub(super) fn follow_parked_demand(&self, serial: u64, index: usize) {
        let demand = {
            let queues = self.lock();
            queues
                .parked
                .get(index)
                .filter(|slot| slot.serial == serial)
                .and_then(ParkedService::demand)
        };
        if let Some((identity, interest)) = demand {
            follow_demand(&identity, &interest);
        }
    }

    /// Close external gates before waiting for logical admission to drain.
    /// Workers remain live until all resumed cleanup and ordinary work finish.
    pub(crate) fn close_suspension(&self) {
        let mut queues = self.lock();
        queues.suspension_closed = true;
        queues.resume_shutdown();
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }

    /// Consumer cancellation and destruction wake only that service's gate.
    /// The flag is set before this lock; park checks it under this same lock.
    pub(crate) fn resume_cancelled(&self, identity: &Arc<AtomicU8>) {
        let mut queues = self.lock();
        let index = queues.parked.iter().position(|slot| {
            slot.work
                .as_ref()
                .and_then(Work::service_identity)
                .is_some_and(|candidate| Arc::ptr_eq(candidate, identity))
        });
        if let Some(index) = index {
            queues.resume(index);
            self.publish_queued(&queues);
            self.notify_ready(queues);
        }
    }
}

impl ReadySink for Dispatch {
    fn signal(self: Arc<Self>, epoch: u64, input: usize, _outcome: JobOutcome) {
        let mut queues = self.lock();
        if !queues
            .parked
            .get(input)
            .is_some_and(|slot| slot.serial == epoch && slot.work.is_some())
        {
            return;
        }
        queues.resume(input);
        self.publish_queued(&queues);
        self.notify_ready(queues);
    }
}
