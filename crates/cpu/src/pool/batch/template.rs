//! Validated reusable dependency structure; epoch inputs and results stay typed.

use super::{FrameBatch, FrameBatchPlan};
use crate::{CpuError, CpuExecutor, ReadyToken};
use std::ops::Range;

/// A stable dependency topology independent of any live input/result generation.
/// Operations remain registered on the typed batch. Independent phases need no
/// per-node template allocation; dependent templates retain flat validated edges.
pub struct FrameGraphTemplate {
    plan: FrameBatchPlan,
    ranges: Vec<Range<usize>>,
    parents: Vec<usize>,
}

impl FrameGraphTemplate {
    /// Describes independent active inputs without allocating or scanning residents.
    #[must_use]
    pub const fn independent(jobs: usize) -> Self {
        Self {
            plan: FrameBatchPlan::new(jobs, 0),
            ranges: Vec::new(),
            parents: Vec::new(),
        }
    }

    /// Validates a backward-only graph once. Slice position is the local node
    /// identity; it never acts as an asynchronous result handle.
    /// # Errors
    /// Rejects duplicate, self, forward or out-of-range edges, and storage overflow.
    pub fn with_dependencies(prerequisites: &[&[usize]]) -> Result<Self, CpuError> {
        let edges = prerequisites.iter().try_fold(0usize, |sum, parents| {
            sum.checked_add(parents.len()).ok_or(CpuError::BatchStorage)
        })?;
        for (node, parents) in prerequisites.iter().enumerate() {
            for (position, &parent) in parents.iter().enumerate() {
                if parent >= node || parents[..position].contains(&parent) {
                    return Err(CpuError::InvalidGraph);
                }
            }
        }
        let mut template = Self::independent(prerequisites.len());
        if edges == 0 {
            return Ok(template);
        }
        template.plan.edges = edges;
        template
            .ranges
            .try_reserve(prerequisites.len())
            .map_err(|_| CpuError::BatchStorage)?;
        template
            .parents
            .try_reserve(edges)
            .map_err(|_| CpuError::BatchStorage)?;
        for parents in prerequisites {
            let start = template.parents.len();
            template.parents.extend_from_slice(parents);
            template.ranges.push(start..template.parents.len());
        }
        Ok(template)
    }

    /// Returns the exact number of active inputs required by a binding.
    #[must_use]
    pub const fn job_count(&self) -> usize {
        self.plan.jobs
    }

    /// Returns a template-local predecessor slice; only admitted indices reach here.
    fn parents(&self, node: usize) -> &[usize] {
        self.ranges
            .get(node)
            .map_or(&[], |range| &self.parents[range.clone()])
    }
}

impl<T: Send + 'static> FrameBatch<T> {
    /// Opens a phase after every resource or heterogeneous producer succeeds.
    /// The supplied slice is its complete bounded external edge list; no worker
    /// is queued while prerequisites remain unresolved.
    /// # Errors
    /// Rejects duplicate/stale identities, subscriber exhaustion and admission
    /// failure before the caller transfers any owned input.
    pub fn begin_after(
        &mut self,
        cpu: &CpuExecutor,
        plan: FrameBatchPlan,
        dependencies: &[ReadyToken],
    ) -> Result<(), CpuError> {
        self.begin_dependencies(cpu, plan, dependencies)
    }

    /// Binds owned inputs to a validated topology and optional external fan-in.
    /// Reset visits only the activated nodes/edges. All storage is reserved before
    /// draining inputs, and workers start after the complete graph is installed.
    /// Template storage may be reused or dropped immediately after binding: the
    /// activation owns its dependency counters and every result generation.
    /// # Errors
    /// Count, capacity and readiness failures leave `jobs` unchanged.
    pub fn start_graph(
        &mut self,
        cpu: &CpuExecutor,
        template: &FrameGraphTemplate,
        jobs: &mut Vec<T>,
        dependencies: &[ReadyToken],
    ) -> Result<(), CpuError> {
        if jobs.len() != template.job_count() {
            return Err(CpuError::GraphInputCount);
        }
        self.begin_dependencies(cpu, template.plan, dependencies)?;
        let mut state = self.core.lock();
        for (node, job) in jobs.drain(..).enumerate() {
            state.append(Some(job), template.parents(node).iter().copied());
        }
        state.open = false;
        let launch = state.runners_to_launch();
        drop(state);
        self.core.launch(launch);
        self.core.finish_if_terminal();
        Ok(())
    }
}
