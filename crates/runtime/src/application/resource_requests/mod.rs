//! Single-owner request authority; domain owners retain their publication order.

mod consumers;
mod service;

use std::{collections::HashMap, sync::Arc};

use solarity_cpu::{CpuService, CpuTask};

#[cfg(test)]
#[path = "../../../tests/application/resource_requests.rs"]
mod tests;

/// One immutable result fans out without duplicating payloads or error objects.
pub(super) type SharedResult<T, E> = Result<Arc<T>, Arc<E>>;

/// Work and results stay owned even when all original consumers withdraw.
enum State<T, E> {
    Requested,
    Running(CpuTask<Result<Arc<T>, E>>),
    Ready(SharedResult<T, E>),
}

/// Counts change at subscription boundaries, never by scanning external Arc pins.
struct Entry<C, T, E> {
    consumers: HashMap<C, CpuService>,
    demand: [usize; 3],
    state: State<T, E>,
    trace: solarity_profiling::TraceContext,
}

impl<C, T, E> Entry<C, T, E> {
    /// Abandoned producers drain as retirement; reacquisition can promote them again.
    fn service(&self) -> CpuService {
        if self.demand[0] != 0 {
            CpuService::Required
        } else if self.demand[1] != 0 || self.consumers.is_empty() {
            CpuService::Retirement
        } else {
            CpuService::Speculative
        }
    }

    /// Changes scheduling metadata only; it never executes or waits for the producer.
    fn update_service(&self) {
        if let State::Running(task) = &self.state {
            task.set_service(self.service());
        }
    }
}

/// Shares each pending key among independently registered consumer generations.
///
/// Keys must include the domain's namespace/options; consumers must identify
/// their exact owner lifetime. Registration retains the caller's domain admission
/// policy; this index adds no capacity or retention substitute. Only admitted
/// producers are polled. Ready values stay until their registered consumers
/// release or cancel them; this is not a retained
/// resource cache or a replacement for the domain's external resource leases.
/// No worker borrows this index, and no map guard covers domain work or disposal.
pub(super) struct ResourceRequests<K, C, T, E> {
    entries: HashMap<K, Entry<C, T, E>>,
    active: Vec<K>,
}

impl<K, C, T, E> ResourceRequests<K, C, T, E> {
    /// Creates an empty index without allocating an unused resource table.
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            active: Vec::new(),
        }
    }

    /// Reports whether a producer still owns domain inputs, including cancelled work.
    pub(super) fn has_running(&self) -> bool {
        !self.active.is_empty()
    }
}

/// Keeps demand counters independent of the CPU enum's representation.
fn demand_index(service: CpuService) -> usize {
    match service {
        CpuService::Required => 0,
        CpuService::Retirement => 1,
        CpuService::Speculative => 2,
    }
}
