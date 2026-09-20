//! Finite model groups amortize dispatch while retaining per-model owned state.

use super::super::super::RuntimeTerrainFrameError;
use super::{GeometryBatch, GeometryJob};
use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, JobCost};
use std::time::Duration;

/// Scheduling bounds never alter scene clocks or model demand. Unknown work runs
/// alone until calibrated; an indivisible expensive model also remains alone.
const MAX_MODELS: usize = 16;
const TARGET: Duration = Duration::from_micros(100);

/// Charged model records move as one scheduler node; each keeps its own output pages.
#[derive(Default)]
pub(super) struct GeometryChunk {
    pub(super) jobs: solarity_cpu::CpuBuffer<GeometryJob>,
    estimated: Duration,
    unknown: bool,
}

impl GeometryChunk {
    /// Reserve before any model relinquishes its mutable simulation state.
    pub(super) fn reserve(
        &mut self,
        budget: &CpuStorageBudget,
    ) -> Result<(), solarity_cpu::CpuError> {
        self.jobs.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Scratch,
            MAX_MODELS,
        )
    }

    /// Separate unknown/heavy work and stop before exceeding the estimated quantum.
    pub(super) fn precedes(&self, next: JobCost) -> bool {
        !self.jobs.is_empty()
            && (self.unknown
                || self.jobs.len() == MAX_MODELS
                || next
                    .duration()
                    .is_none_or(|cost| self.estimated.saturating_add(cost) > TARGET))
    }

    /// Admission already proved capacity, so the transfer cannot lose owned state.
    pub(super) fn push(&mut self, job: GeometryJob, cost: JobCost) {
        assert!(
            self.jobs.len() < MAX_MODELS && self.jobs.len() < self.jobs.capacity(),
            "draw chunk owns reserved model capacity before transfer"
        );
        self.unknown |= cost.duration().is_none();
        if let Some(cost) = cost.duration() {
            self.estimated = self.estimated.saturating_add(cost);
        }
        self.jobs
            .push(job)
            .unwrap_or_else(|_| unreachable!("reserved draw chunk accepts its model"));
    }

    /// Full or uncalibrated groups can release workers before more admission runs.
    pub(super) fn full(&self) -> bool {
        self.jobs.len() == MAX_MODELS || self.unknown || self.estimated >= TARGET
    }

    pub(super) fn cost(&self) -> JobCost {
        if self.unknown {
            JobCost::default()
        } else {
            JobCost::measured(self.estimated)
        }
    }

    /// Independent kernels share dispatch only. No job waits for another job or main.
    pub(super) fn execute(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        context.diagnostic_value("m2.geometry.chunk_models", self.jobs.len() as u64);
        // Admission transferred living simulation state. Complete its ordered
        // model updates even if the consumer withdraws; never discard half a tick.
        for job in self.jobs.writer().iter_mut() {
            job.execute(context);
        }
        solarity_cpu::JobOutcome::Succeeded
    }

    /// Ordered reclamation returns every model, including failed/unexecuted jobs.
    pub(super) fn reclaim(&mut self, jobs: &mut Vec<GeometryJob>) {
        jobs.extend(self.jobs.drain());
        self.estimated = Duration::ZERO;
        self.unknown = false;
    }
}

impl GeometryBatch {
    /// Transfer a complete group transactionally; refusal retains every input.
    pub(super) fn flush_staged(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        if self.staged.jobs.is_empty() {
            return Ok(());
        }
        let cost = self.staged.cost();
        let replacement = self.spare_chunks.pop().unwrap_or_default();
        let mut owned = Some(std::mem::replace(&mut self.staged, replacement));
        match self.pending.push_with_cost(&mut owned, cost) {
            Ok(handle) => self.handles.push(handle),
            Err(error) => {
                let retained =
                    owned.unwrap_or_else(|| unreachable!("refused chunk keeps its state"));
                self.spare_chunks
                    .push(std::mem::replace(&mut self.staged, retained));
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_draw_chunks.rs"]
mod tests;
