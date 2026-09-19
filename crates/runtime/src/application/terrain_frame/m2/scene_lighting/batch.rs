//! Bounded receiver fan-out with ordered publication and unconditional pin return.

use super::work::{LightingInput, LightingWork};
use super::{RuntimeTerrainFrameError, SceneLighting};
use solarity_cpu::{CpuError, FrameBatch, FrameGraphTemplate, FramePriority};
use solarity_rendering::{M2DirectionalLight, M2SceneUniform};
use std::sync::Arc;

/// Initial partition until receiver execution supplies a usable calibration.
const RECEIVERS_PER_BATCH: usize = 64;
const BATCHES_PER_WORKER: usize = 8;
/// Scheduling quantum, not a gameplay time step or an execution deadline.
const TARGET_QUANTUM: std::time::Duration = std::time::Duration::from_micros(100);

/// The active phase owns all mutable outputs; completed frames retain reusable slots.
pub(super) struct LightingBatch {
    pending: FrameBatch<LightingWork>,
    jobs: Vec<LightingWork>,
    submitted: bool,
    calibration: solarity_cpu::CostCalibration,
    costs: solarity_cpu::CpuBuffer<solarity_cpu::JobCost>,
}

impl Default for LightingBatch {
    fn default() -> Self {
        Self {
            pending: FrameBatch::with_outcome(LightingWork::execute),
            jobs: Vec::new(),
            submitted: false,
            calibration: solarity_cpu::CostCalibration::default(),
            costs: solarity_cpu::CpuBuffer::default(),
        }
    }
}

impl SceneLighting {
    /// Main-only consumers subscribe to this phase without taking its output or
    /// extending mutable receiver ownership beyond the existing batch lifecycle.
    pub(in crate::application::terrain_frame::m2) fn completion(
        &self,
    ) -> Result<Option<solarity_cpu::ReadyToken>, CpuError> {
        self.batch
            .submitted
            .then(|| self.batch.pending.completion())
            .transpose()
    }

    pub(in crate::application::terrain_frame::m2) fn has_pending(&self) -> bool {
        self.batch.submitted
    }

    pub(in crate::application::terrain_frame::m2) fn is_ready(&self) -> bool {
        !self.batch.submitted || self.batch.pending.is_finished()
    }

    /// The main driver parks only after running its other permitted continuations.
    pub(in crate::application::terrain_frame::m2) fn wait_pending(
        &self,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_reclaim(&self.batch.pending)?;
        Ok(())
    }

    /// Admits the full output bound before sharing inputs. Callbacks and receiver
    /// identities stay ordered on main; independent uniform ranges can finish freely.
    pub(in crate::application::terrain_frame::m2) fn begin_finish(
        &mut self,
        cpu: Option<&solarity_cpu::CpuExecutor>,
        dependency: Option<&solarity_cpu::ReadyToken>,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let dependencies = match cpu {
            Some(_) => std::slice::from_ref(dependency.ok_or(CpuError::BatchInactive)?),
            None => &[],
        };
        if self.batch.submitted || self.batch.jobs.iter().any(|job| job.input.is_some()) {
            return Err(CpuError::BatchActive.into());
        }
        let count = self.receivers.len();
        let jobs = cpu.map_or(1, |cpu| {
            count
                .div_ceil(
                    self.batch
                        .calibration
                        .units_for(TARGET_QUANTUM)
                        .unwrap_or(RECEIVERS_PER_BATCH),
                )
                .max(1)
                .min(cpu.worker_count().saturating_mul(BATCHES_PER_WORKER))
        });
        let width = count.div_ceil(jobs);
        self.batch.costs.clear();
        if let Some(cpu) = cpu {
            // Hint storage is admitted before receiver/source pins or scene storage move.
            self.batch.costs.reserve(
                cpu.storage(),
                solarity_cpu::CpuStorageClass::Frame,
                solarity_cpu::CpuStorageKind::Metadata,
                jobs,
            )?;
        }
        self.batch.jobs.resize_with(jobs, LightingWork::default);
        // Main publication requires a contiguous ordered array for scene indices.
        self.scenes
            .try_reserve(count.saturating_sub(self.scenes.len()))
            .map_err(|_| CpuError::StorageAllocation)?;
        for job in &mut self.batch.jobs {
            job.scenes.clear();
            job.result = None;
            if jobs != 1 {
                job.scenes
                    .try_reserve(width)
                    .map_err(|_| CpuError::StorageAllocation)?;
            }
        }
        self.prepare_directionals();
        if jobs == 1 {
            // Retain the original no-copy transfer for the common small scene.
            std::mem::swap(&mut self.scenes, &mut self.batch.jobs[0].scenes);
        }
        for (index, job) in self.batch.jobs.iter_mut().enumerate() {
            let start = (index * width).min(count);
            job.measurement = self
                .batch
                .calibration
                .prepare((start + width).min(count) - start);
            job.input = Some(LightingInput {
                sources: Arc::clone(&self.sources),
                receivers: Arc::clone(&self.receivers),
                range: start..(start + width).min(count),
                base,
                exterior,
            });
        }
        solarity_profiling::profile_value!("m2.scene_lighting.batches", jobs);
        solarity_profiling::profile_value!("m2.scene_lighting.receivers", count);
        if let Some(cpu) = cpu {
            for job in &self.batch.jobs {
                self.batch.costs.push(job.measurement.cost())?;
            }
            let template =
                FrameGraphTemplate::independent(jobs).with_priority(FramePriority::Prerequisite);
            if let Err(error) = self.batch.pending.start_costed_graph(
                cpu,
                &template,
                &mut self.batch.jobs,
                dependencies,
                &self.batch.costs,
            ) {
                for job in &mut self.batch.jobs {
                    job.input = None;
                }
                if jobs == 1 {
                    std::mem::swap(&mut self.scenes, &mut self.batch.jobs[0].scenes);
                }
                return Err(error.into());
            }
            self.batch.submitted = true;
        } else {
            self.batch.jobs[0].execute();
        }
        Ok(())
    }

    /// Returns every input pin before surfacing any error. Only the ordered output
    /// prefix through the first failed batch becomes visible, even if later jobs ran.
    pub(in crate::application::terrain_frame::m2) fn finish_pending(
        &mut self,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let readiness = wait.before_reclaim(&self.batch.pending);
        let worker = if self.batch.submitted {
            self.batch.submitted = false;
            self.batch.pending.reclaim(&mut self.batch.jobs)
        } else {
            Ok(())
        };
        for job in &mut self.batch.jobs {
            self.batch.calibration.record(&mut job.measurement);
            job.input = None;
        }
        let mut result = Ok(());
        let mut publish = true;
        let single = self.batch.jobs.len() == 1;
        for job in &mut self.batch.jobs {
            if publish {
                if single {
                    std::mem::swap(&mut self.scenes, &mut job.scenes);
                } else {
                    // Admission reserved the complete final range before any input pin.
                    solarity_cpu::FixedWriter::new(&mut self.scenes)
                        .extend_from_slice(&job.scenes)?;
                }
                match job.result.take() {
                    Some(Ok(())) => {}
                    Some(Err(error)) => {
                        result = Err(error);
                        publish = false;
                    }
                    None => publish = false,
                }
            }
            job.scenes.clear();
            job.result = None;
        }
        result?;
        readiness?;
        worker?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/scene_lighting_batches.rs"]
mod tests;
