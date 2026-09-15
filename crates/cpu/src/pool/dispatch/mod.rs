//! Persistent protected/flexible workers and durable ready-queue predicates.

mod queues;
mod startup;
mod worker;

use crate::storage::StorageDeque;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Condvar, Mutex};

use super::{CpuError, CpuService};

/// Reusable typed work is erased only at the queue boundary.
pub(crate) trait ReadyWork: Send + Sync {
    /// Executes an admitted portion without waiting for another worker job.
    fn run(self: Arc<Self>, flexible: bool);
    /// Reads phase urgency without acquiring its scheduler lock.
    fn urgent(&self) -> bool;
    /// Runs bounded metadata propagation instead of a domain kernel.
    fn propagate(&self, epoch: u64);
}

/// Eligibility keeps blocking service off the protected frame workers.
#[derive(Clone, Copy)]
pub(crate) enum WorkClass {
    Frame,
    Background(CpuService),
    Priority,
}

/// Cold background closures and retained frame operations share thread ownership.
pub(crate) enum Work {
    Once(Arc<AtomicU8>, Box<dyn FnOnce() + Send>),
    Retained(Arc<dyn ReadyWork>),
    Priority(Arc<dyn ReadyWork>, u64),
}

impl Work {
    /// Runs outside every scheduler lock.
    fn run(self, flexible: bool) {
        match self {
            Self::Once(_, operation) => operation(),
            Self::Retained(operation) => operation.run(flexible),
            Self::Priority(operation, epoch) => operation.propagate(epoch),
        }
    }
    /// Queue classification may read only atomic urgency, never an epoch lock.
    fn urgent(&self) -> bool {
        matches!(self, Self::Retained(operation) if operation.urgent())
    }
}

/// Both queue predicates and shutdown are changed under the same mutex.
struct Queues {
    frame: StorageDeque<Work>,
    urgent: StorageDeque<Work>,
    priority: StorageDeque<Work>,
    required: StorageDeque<Work>,
    retirement: StorageDeque<Work>,
    speculative: StorageDeque<Work>,
    stopping: bool,
}

/// The pool owns all handles; no task creates or detaches a thread.
pub(crate) struct Dispatch {
    queues: Mutex<Queues>,
    ready: Condvar,
    queued: AtomicU8,
    // With no protected worker, service must alternate with frame kernels.
    protected: bool,
}
