//! Main-only capture/reclamation brackets the pure worker-owned finalization stage.

use super::super::super::super::{M2Frame, RuntimeTerrainFrameError};
use super::super::super::diagnostics::Work;
use super::super::publication::GeometryPublication;
use super::{FinalizationJob, FinalizedGeometry, OrderedOutput};
use crate::application::frame_pipeline::FrameWait;
use solarity_cpu::{
    CpuError, CpuExecutor, CpuOwnedCell, CpuStorageClass as Class, CpuStorageKind as Kind,
};
use solarity_rendering::M2TransparentPass;

impl M2Frame {
    /// Admits capacity before a worker can own any inputs. A start refusal keeps
    /// captured inputs in the retained cell for the ordinary abandonment path.
    pub(in crate::application::terrain_frame::m2::preparation) fn begin_finalization(
        &mut self,
        cpu: &CpuExecutor,
        work: &mut Work,
        first_pass: M2TransparentPass,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.finalization.capture");
        let state = &mut self.geometry_batch.finalization;
        if state.submitted || state.owns_inputs {
            return Err(CpuError::BatchActive.into());
        }
        let counts = super::admission::OutputCounts::collect(&mut self.geometry_batch.jobs)?;
        state
            .retained
            .reserve(cpu.storage(), Class::Frame, Kind::Metadata, 1)?;
        if state.retained.is_empty() {
            state.retained.push(CpuOwnedCell::new_with(
                cpu.storage(),
                Class::Frame,
                Kind::Scratch,
                FinalizationJob::default,
            )?)?;
        }
        state.retained[0].transfer(cpu.storage(), Class::Frame, Kind::Scratch)?;
        // Reserve scheduler nodes/readiness before output growth can consume their headroom.
        state
            .pending
            .begin(cpu, solarity_cpu::FrameBatchPlan::new(1, 0))?;
        // The accounting owner can be reborrowed independently of the M2 fields.
        // Restore it before propagating a fallible admission result.
        let job = state.retained[0].value_mut();
        let mut memory = std::mem::take(&mut job.memory);
        let mut sorting = std::mem::take(&mut job.sorting);
        let admitted = memory.prepare(self, cpu.storage(), &mut sorting, counts);
        let job = self.geometry_batch.finalization.retained[0].value_mut();
        job.memory = memory;
        job.sorting = sorting;
        if let Err(error) = admitted {
            let state = &mut self.geometry_batch.finalization;
            state.pending.close();
            // No input was published: retiring this empty epoch cannot run domain work.
            let _ = state.pending.reclaim_into(&mut state.retained.writer());
            return Err(error);
        }

        let mut cell = self
            .geometry_batch
            .finalization
            .retained
            .pop()
            .unwrap_or_else(|| unreachable!("finalization cell was admitted"));
        let job = cell.value_mut();
        job.streams.exchange(self);
        std::mem::swap(&mut job.jobs, &mut self.geometry_batch.jobs);
        job.work = std::mem::take(work);
        job.first_pass = Some(first_pass);
        job.result = None;
        let state = &mut self.geometry_batch.finalization;
        state.owns_inputs = true;
        let mut owned = Some(cell);
        if let Err(error) = state.pending.push(&mut owned) {
            state.retained.push(
                owned.unwrap_or_else(|| unreachable!("refused finalization keeps its input")),
            )?;
            state.pending.close();
            let _ = state.pending.reclaim_into(&mut state.retained.writer());
            return Err(error.into());
        }
        state.pending.close();
        state.submitted = true;
        Ok(())
    }

    /// Readiness is a phase boundary; no result borrow escapes to main.
    pub(in crate::application::terrain_frame::m2::preparation) fn finalization_is_finished(
        &self,
    ) -> bool {
        let state = &self.geometry_batch.finalization;
        !state.submitted || state.pending.is_finished()
    }

    /// The outer frame driver parks only after exhausting independent main work.
    pub(in crate::application::terrain_frame::m2::preparation) fn wait_finalization(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let state = &self.geometry_batch.finalization;
        if state.submitted {
            wait.before_reclaim(&state.pending)?;
        }
        Ok(())
    }

    /// Restores every retained stream and palette even after panic, refusal or
    /// domain error. Domain errors retain precedence over the generic failed job.
    pub(in crate::application::terrain_frame::m2::preparation) fn finish_finalization(
        &mut self,
        wait: &mut FrameWait<'_>,
        work: Option<&mut Work>,
    ) -> Result<Option<FinalizedGeometry>, RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.finalization.consume");
        let state = &mut self.geometry_batch.finalization;
        if !state.owns_inputs {
            return Ok(None);
        }
        let was_submitted = state.submitted;
        let readiness = if was_submitted {
            wait.before_reclaim(&state.pending)
        } else {
            Ok(())
        };
        let reclaimed = if was_submitted {
            state.pending.reclaim_into(&mut state.retained.writer())
        } else {
            Ok(())
        };
        state.submitted = false;
        let mut cell = state
            .retained
            .pop()
            .unwrap_or_else(|| unreachable!("terminal finalization returns its owned cell"));
        let job = cell.value_mut();
        job.streams.exchange(self);
        std::mem::swap(&mut job.jobs, &mut self.geometry_batch.jobs);
        if let Some(work) = work {
            std::mem::swap(work, &mut job.work);
        } else {
            drop(std::mem::take(&mut job.work));
        }
        let result = job.result.take();
        job.first_pass = None;
        let state = &mut self.geometry_batch.finalization;
        state.retained.push(cell)?;
        state.owns_inputs = false;
        let output = result.transpose()?;
        readiness?;
        reclaimed?;
        match output {
            Some(OrderedOutput {
                water,
                vertex_capacity,
                index_capacity,
            }) => Ok(Some(FinalizedGeometry {
                water,
                capacities: GeometryPublication {
                    vertex_capacity,
                    index_capacity,
                },
            })),
            None if !was_submitted => Ok(None),
            None => Err(CpuError::CompletionLost.into()),
        }
    }
}
