//! Flat reserved dependency metadata; no user operation runs under this lock.

use super::ready::ReadyJobs;
use super::{FrameBatchPlan, JobOutcome};
use crate::completion::Subscription;
use crate::pool::dispatch::Dispatch;
use crate::pool::observation::ObservedGuard;
use crate::pool::worker::WorkerLease;
use crate::storage::{StorageDeque, StorageVec};
use crate::{CompletionPort, CoordinatorNotifier, CpuError, ReadyToken};
use crate::{CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};

/// Registered adapters introduce no per-activation boxed closure.
pub(super) enum Kernel<T> {
    Ordinary(fn(&mut T)),
    Reported(fn(&mut T) -> JobOutcome),
    Contextual(fn(&mut T, &crate::JobContext<'_>) -> JobOutcome),
}
impl<T> Copy for Kernel<T> {}
impl<T> Clone for Kernel<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Kernel<T> {
    /// Runs only with exclusively leased state and no scheduler guard.
    pub fn run(self, input: &mut T, context: Option<&crate::JobContext<'_>>) -> JobOutcome {
        match self {
            Self::Ordinary(operation) => {
                operation(input);
                JobOutcome::Succeeded
            }
            Self::Reported(operation) => operation(input),
            Self::Contextual(operation) => operation(
                input,
                context.unwrap_or_else(|| {
                    unreachable!("context kernel has admitted cancellation storage")
                }),
            ),
        }
    }

    /// Legacy kernels need no cancellation-page clone during execution.
    pub fn uses_context(self) -> bool {
        matches!(self, Self::Contextual(_))
    }
}

/// One flat record per live dependency, rather than one allocation per owner.
pub(super) struct Edge {
    child: usize,
    next: Option<usize>,
}
/// Distinguishes claimed work from ready records invalidated by cancellation.
#[derive(Clone, Copy)]
pub(super) enum Status {
    Waiting,
    Ready,
    Running,
    Terminal(JobOutcome),
}
/// A phase waits for all declared resource/other-batch prerequisites.
#[derive(Clone, Copy)]
pub(super) enum Gate {
    Ready,
    Pending(usize),
    Failed,
}
/// One admitted node and its dependency propagation state.
pub(super) struct Node {
    pub status: Status,
    pub cancel_requested: bool,
    remaining: usize,
    failed_parent: bool,
    first_edge: Option<usize>,
    pub cost: crate::JobCost,
    pub next_ready: Option<usize>,
}
/// Reusable activation storage; capacity is checked before ownership transfer.
pub(super) struct State<T> {
    pub jobs: StorageVec<Option<T>>,
    pub nodes: StorageVec<Node>,
    pub cancellation: Arc<StorageVec<AtomicBool>>,
    edges: StorageVec<Edge>,
    pub edge_count: usize,
    pub ready: ReadyJobs,
    propagation: StorageDeque<usize>,
    pub generation: u64,
    pub plan: FrameBatchPlan,
    pub runners: usize,
    pub workers: usize,
    pub service: Option<Arc<crate::pool::task::ServiceIdentity>>,
    pub open: bool,
    pub gate: Gate,
    pub subscriptions: StorageVec<Option<Subscription>>,
    pub dependencies: StorageVec<ReadyToken>,
    pub priority_pending: bool,
    pub completion: Option<ReadyToken>,
    pub finishing: bool,
    pub kernel: Kernel<T>,
    pub lease: Option<WorkerLease>,
    pub dispatch: Option<Arc<Dispatch>>,
    pub notifier: Option<Arc<dyn CoordinatorNotifier>>,
    pub trace: solarity_profiling::TraceContext,
    pub drain_tail: super::diagnostics::DrainTail,
}
impl<T> State<T> {
    /// Defers scene-sized storage until a phase declares its bounds.
    pub fn new(kernel: Kernel<T>) -> Self {
        Self {
            jobs: StorageVec::default(),
            nodes: StorageVec::default(),
            cancellation: Arc::new(StorageVec::default()),
            edges: StorageVec::default(),
            edge_count: 0,
            ready: ReadyJobs::default(),
            propagation: StorageDeque::default(),
            generation: 0,
            plan: FrameBatchPlan::default(),
            runners: 0,
            workers: 0,
            service: None,
            open: false,
            gate: Gate::Ready,
            subscriptions: StorageVec::default(),
            dependencies: StorageVec::default(),
            priority_pending: false,
            completion: None,
            finishing: false,
            kernel,
            lease: None,
            dispatch: None,
            notifier: None,
            trace: solarity_profiling::TraceContext::default(),
            drain_tail: super::diagnostics::DrainTail::default(),
        }
    }
    /// Reserves all metadata and propagation storage. Nested T allocations remain
    /// domain-owned; this reservation makes no claim about their byte budget.
    pub fn reserve(
        &mut self,
        plan: FrameBatchPlan,
        dependencies: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<(), CpuError> {
        let kind = CpuStorageKind::Metadata;
        self.jobs.reserve(budget, class, kind, plan.jobs)?;
        self.nodes.reserve(budget, class, kind, plan.jobs)?;
        if self.kernel.uses_context() {
            let cancellation = Arc::get_mut(&mut self.cancellation).unwrap_or_else(|| {
                unreachable!("prior kernels release context before epoch reclamation")
            });
            cancellation.reserve(budget, class, kind, plan.jobs)?;
            cancellation.resize_with(plan.jobs, || AtomicBool::new(false));
        }
        self.edges.reserve(budget, class, kind, plan.edges)?;
        self.propagation.reserve(budget, class, kind, plan.jobs)?;
        self.dependencies
            .reserve(budget, class, kind, dependencies)?;
        self.subscriptions
            .reserve(budget, class, kind, dependencies)?;
        Ok(())
    }
    /// Registers validated parents and atomically observes any terminal outcome.
    pub fn append(
        &mut self,
        job: Option<T>,
        cost: crate::JobCost,
        parents: impl ExactSizeIterator<Item = usize>,
    ) {
        let index = self.jobs.len();
        if self.kernel.uses_context() {
            self.cancellation[index].store(false, std::sync::atomic::Ordering::Release);
        }
        let mut node = Node {
            status: Status::Waiting,
            cancel_requested: false,
            remaining: 0,
            failed_parent: false,
            first_edge: None,
            cost,
            next_ready: None,
        };
        self.edge_count += parents.len();
        for parent in parents {
            match self.nodes[parent].status {
                Status::Terminal(outcome) => node.failed_parent |= outcome != JobOutcome::Succeeded,
                _ => {
                    node.remaining += 1;
                    let next = self.nodes[parent].first_edge;
                    self.nodes[parent].first_edge = Some(self.edges.len());
                    self.edges.push(Edge { child: index, next });
                }
            }
        }
        if node.remaining == 0 {
            if node.failed_parent || matches!(self.gate, Gate::Failed) {
                node.status = Status::Terminal(JobOutcome::DependencyFailed);
            } else {
                node.status = Status::Ready;
            }
        }
        self.jobs.push(job);
        self.nodes.push(node);
        if matches!(self.nodes[index].status, Status::Ready) {
            self.ready.push(&mut self.nodes, index);
        }
    }
    /// Releases transitive successors without recursion or main-thread polling.
    /// Each registered edge is visited at most once.
    pub fn complete(&mut self, index: usize, outcome: JobOutcome) {
        self.nodes[index].status = Status::Terminal(outcome);
        self.propagation.push_back(index);
        while let Some(parent) = self.propagation.pop_front() {
            let Status::Terminal(outcome) = self.nodes[parent].status else {
                unreachable!("propagation contains terminal nodes")
            };
            let mut edge = self.nodes[parent].first_edge.take();
            while let Some(current) = edge {
                let entry = &self.edges[current];
                edge = entry.next;
                let child = &mut self.nodes[entry.child];
                if matches!(child.status, Status::Terminal(_)) {
                    continue;
                }
                child.remaining -= 1;
                child.failed_parent |= outcome != JobOutcome::Succeeded;
                if child.remaining == 0 {
                    if child.failed_parent {
                        child.status = Status::Terminal(JobOutcome::DependencyFailed);
                        self.propagation.push_back(entry.child);
                    } else {
                        child.status = Status::Ready;
                        self.ready.push(&mut self.nodes, entry.child);
                    }
                }
            }
        }
    }
    /// Splits metadata borrows before taking the most expensive ready node.
    pub fn pop_ready(&mut self) -> Option<usize> {
        self.ready.pop(&mut self.nodes)
    }
    /// Removes only cancelled heads; each stale node is discarded at most once.
    pub fn ready_cost(&mut self) -> u8 {
        self.ready.cost(&mut self.nodes)
    }
    /// Reserves dispatch entries before unlocking completion/admission metadata.
    pub fn runners_to_launch(&mut self) -> usize {
        if matches!(self.gate, Gate::Pending(_)) {
            return 0;
        }
        let launch = self
            .ready
            .len()
            .min(self.workers.saturating_sub(self.runners));
        self.runners += launch;
        launch
    }
    /// Identifies the final publication owner without releasing admission early.
    pub fn can_finish(&self) -> bool {
        !self.open
            && self.runners == 0
            && self.ready.is_empty()
            && !matches!(self.gate, Gate::Pending(_))
            && self.lease.is_some()
            && !self.finishing
            && !self.priority_pending
    }
    /// No kernel can have started behind a pending gate. Cancellation suppresses
    /// every waiting input and removes its subscription independently of others.
    pub fn fail_gate(&mut self) {
        self.gate = Gate::Failed;
        self.subscriptions.clear();
        for index in 0..self.nodes.len() {
            if !matches!(self.nodes[index].status, Status::Terminal(_)) {
                self.complete(index, JobOutcome::DependencyFailed);
            }
        }
        self.ready.clear();
    }
    /// Clears activation lengths while retaining metadata for later frames.
    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.nodes.clear();
        self.edges.clear();
        self.edge_count = 0;
        self.ready.clear();
        self.dispatch = None;
        self.service = None;
        self.subscriptions.clear();
        self.drain_tail = super::diagnostics::DrainTail::default();
    }

    /// Running kernels observe one atomic cell without taking the phase mutex.
    pub fn request_cancel(&mut self, index: usize) {
        self.nodes[index].cancel_requested = true;
        if self.kernel.uses_context() {
            self.cancellation[index].store(true, std::sync::atomic::Ordering::Release);
        }
    }
}
/// Shared only with this epoch's workers, never mutable world state.
pub(super) struct Core<T> {
    pub urgent: AtomicBool,
    /// Highest ready bin (0..=2); empty phases use the cheapest dispatch class.
    pub cost: std::sync::atomic::AtomicU8,
    // Zero is inactive. The registry observes this independent metadata owner so
    // pruning cannot retain/drop this Core or lock its domain-bearing state.
    pub live_epoch: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub state: Mutex<State<T>>,
    pub ready: Condvar,
    pub completion_port: CompletionPort,
}
impl<T> Core<T> {
    /// Metadata mutations never execute a domain kernel or consumer callback.
    pub fn lock(&self) -> ObservedGuard<'_, State<T>> {
        static WAIT: solarity_profiling::Site =
            solarity_profiling::Site::new("cpu.batch.lock_wait", true);
        ObservedGuard::lock(&self.state, &WAIT)
    }
}
