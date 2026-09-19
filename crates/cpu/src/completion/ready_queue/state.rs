//! Readiness delivery changes numeric metadata only, then wakes outside locks.

use crate::completion::{CoordinatorNotifier, ReadySink, Subscription};
use crate::storage::{StorageDeque, StorageVec};
use crate::{CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind, JobOutcome};
use std::{
    ops::Range,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

/// A node enters the inbox exactly once, including prerequisite failure.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Status {
    Waiting,
    Ready,
    Delivered,
}

/// The caller's key carries no ownership or executable code.
pub(super) struct Node {
    pub(super) trace: solarity_profiling::TraceContext,
    pub(super) key: u64,
    pub(super) inputs: Range<usize>,
    pub(super) remaining: usize,
    pub(super) status: Status,
    pub(super) outcome: JobOutcome,
}

/// Each input subscription is independent even when two inputs share a producer.
pub(super) struct Input {
    pub(super) node: usize,
    pub(super) delivered: bool,
    pub(super) subscription: Option<Subscription>,
}

/// Queue-to-port is the only nested lock order. Port publication releases its
/// lock before signaling a queue; subscription destruction invokes no callbacks.
pub(super) struct State {
    pub(super) epoch: u64,
    pub(super) active: bool,
    pub(super) node_limit: usize,
    pub(super) input_limit: usize,
    pub(super) delivered: usize,
    pub(super) nodes: StorageVec<Node>,
    pub(super) inputs: StorageVec<Input>,
    pub(super) ready: StorageDeque<usize>,
}

impl State {
    /// Reserves complete metadata bounds, retaining partial successful growth on refusal.
    pub(super) fn reserve(
        &mut self,
        nodes: usize,
        inputs: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<(), CpuError> {
        self.nodes
            .reserve(budget, class, CpuStorageKind::Metadata, nodes)?;
        self.inputs
            .reserve(budget, class, CpuStorageKind::Metadata, inputs)?;
        self.ready
            .reserve(budget, class, CpuStorageKind::Metadata, nodes)
    }

    /// Releases input registrations before reusing their numeric positions.
    pub(super) fn reset(&mut self) {
        self.active = false;
        self.inputs.clear();
        self.nodes.clear();
        self.ready.clear();
        self.delivered = 0;
    }

    /// A failed input releases unrelated waits immediately, so a never-ready
    /// sibling cannot prevent this coordinator from cleaning up its own state.
    pub(super) fn ready(&mut self, node: usize, outcome: JobOutcome) {
        self.nodes[node].status = Status::Ready;
        self.nodes[node].outcome = outcome;
        for input in &mut self.inputs[self.nodes[node].inputs.clone()] {
            input.subscription = None;
        }
        self.ready.push_back(node);
    }
}

/// Workers retain this metadata only for the duration of one notification.
pub(super) struct Core {
    state: Mutex<State>,
    pub(super) changed: Condvar,
    pub(super) notifier: Option<Arc<dyn CoordinatorNotifier>>,
}

impl Core {
    /// Starts without epoch storage; begin performs explicit byte admission.
    pub(super) fn new(notifier: Option<Arc<dyn CoordinatorNotifier>>) -> Self {
        Self {
            changed: Condvar::new(),
            state: Mutex::new(State {
                epoch: 0,
                active: false,
                node_limit: 0,
                input_limit: 0,
                delivered: 0,
                nodes: StorageVec::new(),
                inputs: StorageVec::new(),
                ready: StorageDeque::default(),
            }),
            notifier,
        }
    }

    /// This lock encloses metadata operations only; domain code cannot poison it.
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|_| unreachable!("continuation metadata cannot panic"))
    }

    pub(super) fn notify(&self) {
        self.changed.notify_all();
        if let Some(notifier) = &self.notifier {
            notifier.notify();
        }
    }
}

impl ReadySink for Core {
    fn signal(self: Arc<Self>, epoch: u64, input: usize, outcome: JobOutcome) {
        let mut state = self.lock();
        if !state.active || state.epoch != epoch {
            return;
        }
        let Some(edge) = state.inputs.get_mut(input) else {
            return;
        };
        if edge.delivered {
            return;
        }
        edge.delivered = true;
        let index = edge.node;
        let node = &mut state.nodes[index];
        if node.status != Status::Waiting {
            return;
        }
        node.remaining -= 1;
        let trace = node.trace;
        if outcome != JobOutcome::Succeeded {
            state.ready(index, JobOutcome::DependencyFailed);
        } else if node.remaining == 0 {
            state.ready(index, JobOutcome::Succeeded);
        } else {
            return;
        }
        drop(state);
        trace.link("cpu.main.ready");
        self.notify();
    }
}
