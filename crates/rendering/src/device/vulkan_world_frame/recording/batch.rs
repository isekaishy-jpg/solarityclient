//! Scoped pass admission joins every worker before releasing renderer resources.

use super::super::command::RecordContext;
use super::execution::{WorldFrameExecution, WorldRecordingCompletion};
use super::pools::RecordingPools;
use super::types::{ShadowJob, ShadowSubmission};
use crate::VulkanError;
use solarity_cpu::{CpuExecutor, FrameBatch};
use std::marker::PhantomData;

/// Four stable pass owners retain charged compact command storage between frames.
pub(in super::super) struct ShadowRecording {
    spare: [Option<ShadowJob>; 4],
    jobs: Vec<ShadowJob>,
    batch: FrameBatch<ShadowJob>,
    active: bool,
    calibration: solarity_cpu::CostCalibration,
}

impl Default for ShadowRecording {
    fn default() -> Self {
        Self {
            spare: std::array::from_fn(|_| None),
            jobs: Vec::with_capacity(4),
            batch: FrameBatch::with_context(ShadowJob::execute),
            active: false,
            calibration: solarity_cpu::CostCalibration::default(),
        }
    }
}

/// Resource and pool borrows survive success, failure and unwinding of main recording.
pub(in super::super) struct PendingRecording<'a> {
    recording: &'a mut ShadowRecording,
    _pools: &'a RecordingPools,
    _resources: PhantomData<&'a ()>,
    submission: ShadowSubmission,
}

impl ShadowRecording {
    /// Captures complete commands before dispatch; workers need no renderer registry locks.
    pub(in super::super) fn begin<'a>(
        &'a mut self,
        cpu: &CpuExecutor,
        context: &'a RecordContext<'_>,
        pools: &'a RecordingPools,
    ) -> Result<PendingRecording<'a>, VulkanError> {
        debug_assert!(!self.active && self.jobs.is_empty());
        let _profile = solarity_profiling::profile!("rendering.shadow.capture");
        let mut submission = ShadowSubmission::default();
        let commands = pools.commands();
        for (index, command) in commands.into_iter().enumerate() {
            let mut job = self.spare[index].take().unwrap_or_default();
            let captured = job.capture(context, index, command, cpu.storage());
            match captured {
                Ok(true) => {
                    job.pass_index = index;
                    job.measurement = self.calibration.prepare(job.draws.len());
                    submission.commands[submission.count] = command;
                    submission.count += 1;
                    self.jobs.push(job);
                }
                Ok(false) => self.spare[index] = Some(job),
                Err(error) => {
                    self.spare[index] = Some(job);
                    self.restore();
                    return Err(error);
                }
            }
        }
        if !self.jobs.is_empty() {
            // Pass order remains fixed at submission. Scheduling can use prior
            // successful command-recording costs without sampling every frame.
            let mut costs = [solarity_cpu::JobCost::default(); 4];
            for (cost, job) in costs.iter_mut().zip(&self.jobs) {
                *cost = job.measurement.cost();
            }
            let count = self.jobs.len();
            if let Err(error) = self.batch.start_costed_graph(
                cpu,
                &solarity_cpu::FrameGraphTemplate::independent(count),
                &mut self.jobs,
                &[],
                &costs[..count],
            ) {
                self.restore();
                return Err(error.into());
            }
            self.active = true;
        }
        Ok(PendingRecording {
            recording: self,
            _pools: pools,
            _resources: PhantomData,
            submission,
        })
    }

    /// Each retained allocation returns to its pass, including failed work.
    fn restore(&mut self) {
        for job in self.jobs.drain(..) {
            let index = job.pass_index;
            self.spare[index] = Some(job);
        }
    }

    /// Always joins before exposing a domain result; failure never strands a command-pool owner.
    fn finish(&mut self) -> Result<(), VulkanError> {
        if !self.active {
            return Ok(());
        }
        let joined = self.batch.reclaim(&mut self.jobs);
        self.active = false;
        let mut domain = Ok(());
        for job in &mut self.jobs {
            self.calibration.record(&mut job.measurement);
            if domain.is_ok() {
                domain = job.result.take().unwrap_or_else(|| {
                    if joined.is_err() {
                        Ok(())
                    } else {
                        unreachable!("completed recording job has a domain result")
                    }
                });
            }
        }
        self.restore();
        // The domain error retains the Vulkan operation; CPU-only failures still propagate.
        domain?;
        Ok(joined?)
    }
}

impl PendingRecording<'_> {
    pub(in super::super) fn has_shadows(&self) -> bool {
        self.submission.count != 0
    }

    /// Main has completed independent scene recording before consuming this required product.
    pub(in super::super) fn finish(
        mut self,
        execution: &mut impl WorldFrameExecution,
    ) -> Result<ShadowSubmission, VulkanError> {
        let readiness = if self.recording.active && !self.recording.batch.is_finished() {
            self.recording
                .batch
                .require_urgent()
                .map_err(VulkanError::from)
                .and_then(|()| {
                    let _profile = solarity_profiling::profile!("rendering.shadow.pending");
                    execution.wait_for_recording(&WorldRecordingCompletion {
                        batch: &self.recording.batch,
                    })
                })
        } else {
            Ok(())
        };
        let result = self.recording.finish();
        readiness?;
        result?;
        Ok(std::mem::take(&mut self.submission))
    }
}

impl Drop for PendingRecording<'_> {
    fn drop(&mut self) {
        let _ = self.recording.finish();
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/shadow_recording.rs"]
mod tests;
