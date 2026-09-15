//! Flat reserved dependency metadata; no user operation runs under this lock.

use super::{FrameBatchPlan, FrameJob, JobOutcome};
use crate::pool::dispatch::Dispatch;
use crate::pool::worker::WorkerLease;
use crate::{CoordinatorNotifier, CpuError};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Registered adapters introduce no per-activation boxed closure.
pub(super) enum Kernel<T> {
    Ordinary(fn(&mut T)),
    Reported(fn(&mut T) -> JobOutcome),
}
impl<T> Copy for Kernel<T> {}
impl<T> Clone for Kernel<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Kernel<T> {
    /// Runs only with exclusively leased state and no scheduler guard.
    pub fn run(self, input: &mut T) -> JobOutcome {
        match self {
            Self::Ordinary(operation) => {
                operation(input);
                JobOutcome::Succeeded
            }
            Self::Reported(operation) => operation(input),
        }
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
/// One admitted node and its dependency propagation state.
pub(super) struct Node {
    pub status: Status,
    pub cancel_requested: bool,
    remaining: usize,
    failed_parent: bool,
    first_edge: Option<usize>,
}
/// Reusable activation storage; capacity is checked before ownership transfer.
pub(super) struct State<T> {
    pub jobs: Vec<Option<T>>,
    pub nodes: Vec<Node>,
    edges: Vec<Edge>,
    pub edge_count: usize,
    pub ready: VecDeque<usize>,
    propagation: VecDeque<usize>,
    pub generation: u64,
    pub plan: FrameBatchPlan,
    pub runners: usize,
    pub workers: usize,
    pub open: bool,
    pub kernel: Kernel<T>,
    pub lease: Option<WorkerLease>,
    pub dispatch: Option<Arc<Dispatch>>,
    pub notifier: Option<Arc<dyn CoordinatorNotifier>>,
    pub trace: solarity_profiling::TraceContext,
}
impl<T> State<T> {
    /// Defers scene-sized storage until a phase declares its bounds.
    pub fn new(kernel: Kernel<T>) -> Self {
        Self {
            jobs: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            edge_count: 0,
            ready: VecDeque::new(),
            propagation: VecDeque::new(),
            generation: 0,
            plan: FrameBatchPlan::default(),
            runners: 0,
            workers: 0,
            open: false,
            kernel,
            lease: None,
            dispatch: None,
            notifier: None,
            trace: solarity_profiling::TraceContext::default(),
        }
    }
    /// Reserves all metadata and propagation storage. Nested T allocations remain
    /// domain-owned; this reservation makes no claim about their byte budget.
    pub fn reserve(&mut self, plan: FrameBatchPlan) -> Result<(), CpuError> {
        self.jobs
            .try_reserve(plan.jobs)
            .map_err(|_| CpuError::BatchStorage)?;
        self.nodes
            .try_reserve(plan.jobs)
            .map_err(|_| CpuError::BatchStorage)?;
        self.edges
            .try_reserve(plan.edges)
            .map_err(|_| CpuError::BatchStorage)?;
        self.ready
            .try_reserve(plan.jobs)
            .map_err(|_| CpuError::BatchStorage)?;
        self.propagation
            .try_reserve(plan.jobs)
            .map_err(|_| CpuError::BatchStorage)?;
        Ok(())
    }
    /// Registers validated parents and atomically observes any terminal outcome.
    pub fn append(&mut self, job: Option<T>, parents: &[FrameJob<T>]) {
        let index = self.jobs.len();
        let mut node = Node {
            status: Status::Waiting,
            cancel_requested: false,
            remaining: 0,
            failed_parent: false,
            first_edge: None,
        };
        self.edge_count += parents.len();
        for parent in parents {
            match self.nodes[parent.index].status {
                Status::Terminal(outcome) => node.failed_parent |= outcome != JobOutcome::Succeeded,
                _ => {
                    node.remaining += 1;
                    let next = self.nodes[parent.index].first_edge;
                    self.nodes[parent.index].first_edge = Some(self.edges.len());
                    self.edges.push(Edge { child: index, next });
                }
            }
        }
        if node.remaining == 0 {
            if node.failed_parent {
                node.status = Status::Terminal(JobOutcome::DependencyFailed);
            } else {
                node.status = Status::Ready;
                self.ready.push_back(index);
            }
        }
        self.jobs.push(job);
        self.nodes.push(node);
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
                        self.ready.push_back(entry.child);
                    }
                }
            }
        }
    }
    /// Reserves dispatch entries before unlocking completion/admission metadata.
    pub fn runners_to_launch(&mut self) -> usize {
        let launch = self
            .ready
            .len()
            .min(self.workers.saturating_sub(self.runners));
        self.runners += launch;
        launch
    }
    /// Backward-only dependencies guarantee a closed phase with no active/ready
    /// worker has no unresolved external producer.
    pub fn release_if_terminal(&mut self) {
        if !self.open && self.runners == 0 && self.ready.is_empty() {
            self.lease = None;
        }
    }
    /// Clears activation lengths while retaining metadata for later frames.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.edge_count = 0;
        self.ready.clear();
        self.dispatch = None;
    }
}
/// Shared only with this epoch's workers, never mutable world state.
pub(super) struct Core<T> {
    pub state: Mutex<State<T>>,
    pub ready: Condvar,
}
impl<T> Core<T> {
    /// Metadata mutations never execute a domain kernel or consumer callback.
    pub fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state
            .lock()
            .unwrap_or_else(|_| unreachable!("batch metadata mutations cannot panic"))
    }
}
