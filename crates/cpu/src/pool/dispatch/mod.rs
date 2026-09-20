//! Persistent protected/flexible workers and durable ready-queue predicates.

mod cost;
mod observation;
mod queues;
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
    fn run(self: Arc<Self>, flexible: bool);
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
    /// Returns true only when the same owned operation needs another ready turn.
    fn step(&mut self) -> bool;
}

/// Cold background closures and retained frame operations share thread ownership.
pub(crate) enum Work {
    Once(Arc<AtomicU8>, Box<dyn FnOnce() + Send>),
    Sliced(Arc<AtomicU8>, Box<dyn ServiceStep>),
    Retained(Arc<dyn ReadyWork>),
    Loading(Arc<AtomicU8>, Arc<dyn ReadyWork>),
    Priority(Arc<dyn ReadyWork>, u64),
}

impl Work {
    /// Runs outside every scheduler lock.
    fn run(self, flexible: bool) -> Option<Self> {
        match self {
            Self::Once(_, operation) => operation(),
            Self::Sliced(identity, mut operation) => {
                if operation.step() {
                    return Some(Self::Sliced(identity, operation));
                }
            }
            Self::Retained(operation) | Self::Loading(_, operation) => operation.run(flexible),
            Self::Priority(operation, epoch) => operation.propagate(epoch),
        }
        None
    }

    /// Reads the private service identity without invoking a kernel or trait method.
    fn service_identity(&self) -> Option<&Arc<AtomicU8>> {
        match self {
            Self::Once(identity, _) | Self::Sliced(identity, _) | Self::Loading(identity, _) => {
                Some(identity)
            }
            Self::Retained(_) | Self::Priority(_, _) => None,
        }
    }

    /// Called while queue metadata is locked so reclassification cannot race enqueue.
    fn service(&self) -> CpuService {
        let identity = self
            .service_identity()
            .unwrap_or_else(|| unreachable!("background dispatch owns a service identity"));
        CpuService::from_raw(identity.load(Ordering::Acquire))
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
    required: StorageDeque<QueuedWork>,
    retirement: StorageDeque<QueuedWork>,
    speculative: StorageDeque<QueuedWork>,
    sleepers: crate::storage::StorageVec<SleepingWorker>,
    stopping: bool,
    active_service: usize,
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
