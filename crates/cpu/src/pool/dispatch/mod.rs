//! Persistent protected/flexible workers and durable ready-queue predicates.

mod cost;
mod observation;
mod parking;
mod queues;
mod service;
mod startup;
mod worker;

use crate::storage::StorageDeque;
use observation::{QueuedWork, SleepingWorker};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::{CpuError, CpuService};

/// Reusable typed work is erased only at the queue boundary.
pub(crate) trait ReadyWork: Send + Sync {
    /// Executes an admitted portion without waiting for another worker job.
    fn run(self: Arc<Self>, flexible: bool, worker: WorkerLane);
    /// Reads phase urgency without acquiring its scheduler lock.
    fn urgent(&self) -> bool;
    /// Reads the highest currently ready cost bin (0..=2), without a phase lock.
    fn cost(&self) -> u8;
    /// Runs bounded metadata propagation instead of a domain kernel.
    fn propagate(&self, epoch: u64);
}

/// Eligibility keeps blocking service off the protected frame workers.
#[derive(Clone, Copy)]
pub(crate) enum WorkClass {
    Frame,
    Background,
    Priority,
}

/// One owned finite service operation with resumable domain-defined boundaries.
/// Implementations stay private to the pool; no domain callback runs under queues.
pub(crate) trait ServiceStep: Send {
    /// Runs a domain turn outside dispatch locks.
    fn step(&mut self, worker: WorkerLane) -> ServiceTurn;
    /// Metadata-only withdrawal never invokes the domain or destroys captures.
    fn cancel(&self);
    /// Prevents cancellation racing a new suspension from losing its wake.
    fn is_cancelled(&self) -> bool;
    /// Cold readiness state stays inside the existing service allocation rather
    /// than widening every frame runner in the shared ready queues.
    fn retain_dependency(
        &mut self,
        dependency: crate::CpuTaskDependency,
    ) -> crate::completion::Binding;
    /// Clones scheduling metadata only; never accesses domain captures.
    fn dependency_demand(&self) -> Option<crate::CpuServiceInterest>;
}

/// Internal terminal, runnable and externally gated service transitions.
pub(crate) enum ServiceTurn {
    Finished,
    Ready,
    Waiting(crate::CpuTaskDependency),
}

/// A yielded operation retains its existing admission and owned captures.
pub(super) enum Continuation {
    Ready(Work),
    Waiting(Work, crate::CpuTaskDependency),
}

/// Physical execution identity selects scratch only, never gameplay RNG or job order.
#[derive(Clone, Copy)]
pub(crate) struct WorkerLane {
    pub owner: usize,
    pub index: usize,
    pub environment: crate::environment::WorkerEnvironment,
    pub execution: crate::CpuServiceExecution,
}

/// Cold background closures and retained frame operations share thread ownership.
pub(crate) enum Work {
    Once(
        Arc<crate::pool::task::ServiceIdentity>,
        crate::CpuServiceExecution,
        Box<dyn FnOnce(WorkerLane) + Send>,
    ),
    Sliced(
        Arc<crate::pool::task::ServiceIdentity>,
        crate::CpuServiceExecution,
        Box<dyn ServiceStep>,
    ),
    Retained(Arc<dyn ReadyWork>),
    Loading(Arc<crate::pool::task::ServiceIdentity>, Arc<dyn ReadyWork>),
    Priority(Arc<dyn ReadyWork>, u64),
}

impl Work {
    /// Runs outside every scheduler lock.
    fn run(self, flexible: bool, worker: WorkerLane) -> Option<Continuation> {
        match self {
            Self::Once(_, _, operation) => operation(worker),
            Self::Sliced(identity, execution, mut operation) => match operation.step(worker) {
                ServiceTurn::Finished => {}
                ServiceTurn::Ready => {
                    return Some(Continuation::Ready(Self::Sliced(
                        identity, execution, operation,
                    )));
                }
                ServiceTurn::Waiting(dependency) => {
                    return Some(Continuation::Waiting(
                        Self::Sliced(identity, execution, operation),
                        dependency,
                    ));
                }
            },
            Self::Retained(operation) | Self::Loading(_, operation) => {
                operation.run(flexible, worker)
            }
            Self::Priority(operation, epoch) => operation.propagate(epoch),
        }
        None
    }

    /// Reads the private service identity without invoking a kernel or trait method.
    fn service_identity(&self) -> Option<&Arc<crate::pool::task::ServiceIdentity>> {
        match self {
            Self::Once(identity, ..) | Self::Sliced(identity, ..) | Self::Loading(identity, _) => {
                Some(identity)
            }
            Self::Retained(_) | Self::Priority(_, _) => None,
        }
    }

    /// Loading kernels can contain foreign asset calls; frame kernels are finite.
    fn execution(&self) -> crate::CpuServiceExecution {
        match self {
            Self::Once(_, execution, _) | Self::Sliced(_, execution, ..) => *execution,
            Self::Loading(..) => crate::CpuServiceExecution::Bulk,
            Self::Retained(..) | Self::Priority(..) => crate::CpuServiceExecution::Finite,
        }
    }

    /// A saturated bulk allowance does not occupy an idle finite service lane.
    fn eligible(&self, bulk_available: bool) -> bool {
        bulk_available || self.execution() == crate::CpuServiceExecution::Finite
    }

    /// Called while queue metadata is locked so reclassification cannot race enqueue.
    fn service(&self) -> CpuService {
        let identity = self
            .service_identity()
            .unwrap_or_else(|| unreachable!("background dispatch owns a service identity"));
        CpuService::from_raw(identity.effective.load(Ordering::Acquire))
    }
    /// Queue classification may read only atomic urgency, never an epoch lock.
    fn urgent(&self) -> bool {
        matches!(self, Self::Retained(operation) if operation.urgent())
    }

    /// Only retained frame runners enter cost buckets; other classes have their
    /// own eligibility and fairness policies rather than guessed execution costs.
    fn cost(&self) -> u8 {
        match self {
            Self::Retained(operation) => operation.cost(),
            Self::Once(..) | Self::Sliced(..) | Self::Loading(..) | Self::Priority(..) => {
                unreachable!("only frame runners enter cost queues")
            }
        }
    }

    /// Identity comparisons do not lease or lock domain-bearing phase state.
    fn belongs_to(&self, owner: &Arc<dyn ReadyWork>) -> bool {
        matches!(self, Self::Retained(candidate) if Arc::ptr_eq(candidate, owner))
    }
}

/// Both queue predicates and shutdown are changed under the same mutex.
struct Queues {
    frame: cost::CostQueue,
    urgent: cost::CostQueue,
    priority: StorageDeque<QueuedWork>,
    required: service::ServiceQueue,
    retirement: service::ServiceQueue,
    speculative: service::ServiceQueue,
    sleepers: crate::storage::StorageVec<SleepingWorker>,
    stopping: bool,
    suspension_closed: bool,
    active_bulk: usize,
    // One slot per logical admission, not per worker or discovered source.
    parked: crate::storage::StorageVec<parking::ParkedService>,
}

/// The pool owns all handles; no task creates or detaches a thread.
pub(crate) struct Dispatch {
    queues: Mutex<Queues>,
    ready: Condvar,
    queued: AtomicU8,
    // With no protected worker, service must alternate with frame kernels.
    protected: bool,
    bulk_limit: usize,
    flexible_workers: usize,
}
