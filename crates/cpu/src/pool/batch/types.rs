//! Checked epoch identities, outcomes and reservation limits.

use super::state::Core;
use std::sync::{Arc, Weak};

/// Scheduling urgency never changes execution eligibility or logical output order.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum FramePriority {
    /// Ordinary frame work with no current blocking consumer.
    #[default]
    Pass,
    /// A prerequisite of main-thread consumption or a known critical phase.
    Prerequisite,
}

/// Complete node/edge maxima, including successors needed to release predecessors.
#[derive(Clone, Copy, Default)]
pub struct FrameBatchPlan {
    pub(super) jobs: usize,
    pub(super) edges: usize,
    pub(super) priority: FramePriority,
}
impl FrameBatchPlan {
    /// Declares a connected phase before any owned input enters the scheduler.
    #[must_use]
    pub const fn new(jobs: usize, edges: usize) -> Self {
        Self {
            jobs,
            edges,
            priority: FramePriority::Pass,
        }
    }

    /// Declares urgency before admission; this does not admit background I/O to
    /// protected workers or preempt a currently executing kernel.
    #[must_use]
    pub const fn with_priority(mut self, priority: FramePriority) -> Self {
        self.priority = priority;
        self
    }
}

/// Non-owning typed identity. A weak owner prevents allocation-identity reuse;
/// the generation rejects late handles when the same batch is activated again.
pub struct FrameJob<T> {
    pub(super) owner: Weak<Core<T>>,
    pub(super) generation: u64,
    pub(super) index: usize,
}
impl<T> Clone for FrameJob<T> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            generation: self.generation,
            index: self.index,
        }
    }
}
impl<T> FrameJob<T> {
    /// Creates a token only after its input slot is admitted.
    pub(super) fn new(core: &Arc<Core<T>>, generation: u64, index: usize) -> Self {
        Self {
            owner: Arc::downgrade(core),
            generation,
            index,
        }
    }
}

/// Scheduler-visible outcome; domain error payloads remain in owned job state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobOutcome {
    /// The kernel completed and successors may execute.
    Succeeded,
    /// A declared domain failure prevents successor execution.
    Failed,
    /// A kernel unwound; potentially modified state is retained for cleanup.
    Panicked,
    /// Cancellation prevented execution or was acknowledged on kernel return.
    Cancelled,
    /// At least one predecessor did not succeed.
    DependencyFailed,
}
impl JobOutcome {
    /// Maps execution failure without taking the domain's error payload.
    pub(super) fn result(self) -> Result<(), crate::CpuError> {
        match self {
            Self::Succeeded => Ok(()),
            Self::Failed => Err(crate::CpuError::JobFailed),
            Self::Panicked => Err(crate::CpuError::TaskPanicked),
            Self::Cancelled => Err(crate::CpuError::JobCancelled),
            Self::DependencyFailed => Err(crate::CpuError::DependencyFailed),
        }
    }
}
