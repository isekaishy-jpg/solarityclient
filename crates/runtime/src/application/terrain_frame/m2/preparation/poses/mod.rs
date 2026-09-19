//! Pure unit pose work follows callbacks and precedes ordered scene publication.

mod admission;
mod frame;
mod input;

pub(in crate::application::terrain_frame::m2) use admission::PoseAdmission;

use super::super::{M2BonePose, RuntimeTerrainFrameError};
use crate::application::frame_pipeline::FrameWait;
use input::PoseJob;
use solarity_asset::ResourceLease;

/// Only current dynamic owners retain palettes; scenery residency is not a job list.
pub(in crate::application::terrain_frame::m2) struct PoseBatch {
    jobs: Vec<PoseJob>,
    indices: Vec<Option<usize>>,
    handles: Vec<solarity_cpu::FrameJob<PoseJob>>,
    pending: solarity_cpu::FrameBatch<PoseJob>,
    submitted: bool,
}

impl Default for PoseBatch {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            indices: Vec::new(),
            handles: Vec::new(),
            pending: solarity_cpu::FrameBatch::new(PoseJob::sample),
            submitted: false,
        }
    }
}

impl PoseBatch {
    pub(in crate::application::terrain_frame::m2) fn is_finished(&self) -> bool {
        self.pending.is_finished()
    }

    /// Waits only for closed-phase retirement; state remains owned until finish.
    pub(in crate::application::terrain_frame::m2) fn wait_finished(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_reclaim(&self.pending)?;
        Ok(())
    }

    /// Services main before the next root enters ordered placement mutation.
    pub(in crate::application::terrain_frame::m2) fn wait_for_root(
        &self,
        index: usize,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.submitted
            && let Some(job) = self.indices.get(index).copied().flatten()
        {
            wait.before_result(&self.pending, &self.handles[job])?;
        }
        Ok(())
    }

    /// Allows ordered traversal to yield before mutating a root whose palette is
    /// still running. Terminal failures remain at the original consumer boundary.
    pub(in crate::application::terrain_frame::m2) fn is_ready(
        &self,
        index: usize,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(job) = self.indices.get(index).copied().flatten() else {
            return Ok(true);
        };
        Ok(!self.submitted || self.pending.outcome(&self.handles[job])?.is_some())
    }

    /// Recovers owned inputs on normal publication and when a prior frame failed.
    pub(in crate::application::terrain_frame::m2) fn finish(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.pending.close();
        let readiness = wait.before_reclaim(&self.pending);
        let result = self.pending.reclaim(&mut self.jobs);
        self.submitted = false;
        readiness?;
        result?;
        Ok(())
    }

    /// Reports results actually left unconsumed, rather than assuming every job helped.
    pub(in crate::application::terrain_frame::m2) fn report_consumption(&self) {
        if solarity_profiling::detail_enabled() {
            solarity_profiling::profile_value!("m2.pose_batch.prepared", self.jobs.len());
            solarity_profiling::profile_value!(
                "m2.pose_batch.unconsumed",
                self.jobs.iter().filter(|job| job.unconsumed()).count()
            );
        }
    }

    /// Consumes a current exact-input palette once, transferring its storage.
    pub(in crate::application::terrain_frame::m2) fn take(
        &mut self,
        index: usize,
        model: &ResourceLease<solarity_asset::DecodedM2Model>,
        clock: solarity_rendering::M2AnimationClock,
        view: glam::Mat4,
        overrides: solarity_rendering::M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(job) = self.indices.get(index).copied().flatten() else {
            return Ok(false);
        };
        if self.submitted {
            return self.pending.with_result(&self.handles[job], |job| {
                job.take(model, clock, view, overrides, output)
            })?;
        }
        self.jobs[job].take(model, clock, view, overrides, output)
    }
}
