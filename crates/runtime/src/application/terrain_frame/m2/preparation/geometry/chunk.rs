//! Finite model groups amortize dispatch while retaining per-model owned state.

use super::super::super::RuntimeTerrainFrameError;
use super::{GeometryBatch, GeometryOwner};
use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, JobCost};
use std::time::Duration;

/// Scheduling bounds never alter scene clocks or model demand. Unknown work runs
/// alone until calibrated; an indivisible expensive model also remains alone.
const MAX_MODELS: usize = 16;
const TARGET: Duration = Duration::from_micros(100);

/// Charged model records move as one scheduler node; each keeps its own output pages.
#[derive(Default)]
pub(super) struct GeometryChunk {
    pub(super) jobs: solarity_cpu::CpuBuffer<GeometryOwner>,
    pub(super) scratch: Option<solarity_cpu::CpuWorkerScratch<usize>>,
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
    pub(super) fn push(&mut self, job: GeometryOwner, cost: JobCost) {
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
        if let Err(error) = self.admit_outputs() {
            self.fail_admission(error);
            return solarity_cpu::JobOutcome::Succeeded;
        }
        let scratch = self
            .scratch
            .as_ref()
            .unwrap_or_else(|| unreachable!("admitted draw chunk pins worker scratch"));
        for job in self.jobs.writer().iter_mut() {
            let job = job.job_mut();
            if job.result.is_none() {
                job.execute(context, scratch);
            }
        }
        solarity_cpu::JobOutcome::Succeeded
    }

    /// Every member already owns its output headroom. Allocate all members before
    /// the first numeric kernel so an allocation failure cannot half-tick the group.
    fn admit_outputs(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.geometry.group_admission");
        for owner in self.jobs.iter_mut() {
            owner.job_mut().allocate_admitted()?;
        }
        Ok(())
    }

    /// A refused group returns every unadvanced effect owner. Keep preexisting
    /// layout failures in order and report capacity failure at the first valid model.
    fn fail_admission(&mut self, error: RuntimeTerrainFrameError) {
        let mut error = Some(error);
        for owner in self.jobs.iter_mut() {
            let job = owner.job_mut();
            if job.result.is_none() {
                job.result = Some(Err(error
                    .take()
                    .unwrap_or_else(|| solarity_cpu::CpuError::DependencyFailed.into())));
            }
            job.admission = None;
            job.context = None;
        }
    }

    /// Ordered reclamation returns every model, including failed/unexecuted jobs.
    pub(super) fn reclaim(
        &mut self,
        jobs: &mut impl solarity_cpu::OutputBuffer<GeometryOwner>,
    ) -> Result<(), solarity_cpu::CpuError> {
        jobs.try_reserve_exact(self.jobs.len())?;
        for mut job in self.jobs.drain() {
            job.job_mut().admission = None;
            job.job_mut().context = None;
            jobs.push(job)
                .unwrap_or_else(|_| unreachable!("model return was preflighted"));
        }
        self.scratch = None;
        self.estimated = Duration::ZERO;
        self.unknown = false;
        Ok(())
    }
}

impl GeometryBatch {
    /// Transfer a complete group transactionally; refusal retains every input.
    pub(super) fn flush_staged(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        if self.staged.jobs.is_empty() {
            return Ok(());
        }
        if !self.submitted {
            return Err(solarity_cpu::CpuError::BatchInactive.into());
        }
        let cost = self.staged.cost();
        self.staged.scratch = Some(
            self.particle_scratch
                .as_ref()
                .unwrap_or_else(|| unreachable!("draw admission owns worker scratch"))
                .clone(),
        );
        let replacement = self.spare_chunks.pop().unwrap_or_default();
        let mut owned = Some(std::mem::replace(&mut self.staged, replacement));
        match self.pending.push_with_cost(&mut owned, cost) {
            Ok(_) => (),
            Err(error) => {
                let retained =
                    owned.unwrap_or_else(|| unreachable!("refused chunk keeps its state"));
                self.spare_chunks
                    .push(std::mem::replace(&mut self.staged, retained))
                    .unwrap_or_else(|_| unreachable!("admitted spare chunk capacity"));
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_draw_chunks.rs"]
mod tests;
