//! Pure unit pose work follows callbacks and precedes ordered scene publication.

mod admission;
mod frame;
mod input;

pub(in crate::application::terrain_frame::m2) use admission::PoseAdmission;

use super::super::{M2BonePose, RuntimeTerrainFrameError};
use input::PoseJob;

/// Only current dynamic owners retain palettes; scenery residency is not a job list.
pub(in crate::application::terrain_frame::m2) struct PoseBatch {
    jobs: Vec<PoseJob>,
    indices: Vec<Option<usize>>,
    pending: solarity_cpu::FrameBatch<PoseJob>,
    submitted: bool,
}

impl Default for PoseBatch {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            indices: Vec::new(),
            pending: solarity_cpu::FrameBatch::new(PoseJob::sample),
            submitted: false,
        }
    }
}

impl PoseBatch {
    /// Recovers owned inputs on normal publication and when a prior frame failed.
    pub(in crate::application::terrain_frame::m2) fn finish(
        &mut self,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let result = self.pending.reclaim(&mut self.jobs);
        self.submitted = false;
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
        model: &std::sync::Arc<solarity_asset::DecodedM2Model>,
        clock: solarity_rendering::M2AnimationClock,
        view: glam::Mat4,
        overrides: solarity_rendering::M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(job) = self.indices.get(index).copied().flatten() else {
            return Ok(false);
        };
        if self.submitted {
            return self
                .pending
                .with_result(job, |job| job.take(model, clock, view, overrides, output))?;
        }
        self.jobs[job].take(model, clock, view, overrides, output)
    }
}
