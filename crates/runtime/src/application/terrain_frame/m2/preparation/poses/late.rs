//! A discovered ordered bone dependency uses the existing pose kernel and frame executor.
use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use super::input::PoseJob;
use crate::application::frame_pipeline::FrameWait;
use solarity_asset::{DecodedM2Model, ResourceLease};
use solarity_cpu::{CpuExecutor, FrameBatch, FrameGraphTemplate, FramePriority};
use solarity_rendering::{M2AnimationClock, M2BonePose, M2BonePoseOverrides, M2BoneSamples};

/// One ordered consumer can be suspended; reusable buffers do not pin its source after the frame.
pub(in crate::application::terrain_frame::m2) struct LatePose {
    jobs: Vec<PoseJob>,
    pending: FrameBatch<PoseJob>,
    calibration: solarity_cpu::CostCalibration,
    active: bool,
    #[cfg(test)]
    consumed: usize,
}
impl Default for LatePose {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            pending: FrameBatch::with_context(PoseJob::execute),
            calibration: solarity_cpu::CostCalibration::default(),
            active: false,
            #[cfg(test)]
            consumed: 0,
        }
    }
}
impl LatePose {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn start(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
        palette: bool,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.active {
            return Err(solarity_cpu::CpuError::BatchActive.into());
        }
        if self.jobs.is_empty() {
            self.jobs
                .push(PoseJob::new(ResourceLease::clone(&source.model)));
        }
        let job = &mut self.jobs[0];
        job.prepare(
            index,
            source,
            clock,
            view,
            overrides.finger_pose,
            overrides.bone_transforms,
            overrides.bone_sequences.to_vec(),
        );
        if !palette {
            job.request_samples(bones, cpu.storage())?;
        }
        job.admit(cpu)?;
        job.measurement = self.calibration.prepare(if palette {
            source.model.animations().bones().len()
        } else {
            bones.len()
        });
        let cost = job.measurement.cost();
        self.pending.start_costed_graph(
            cpu,
            &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
            &mut self.jobs,
            &[],
            &[cost],
        )?;
        self.active = true;
        Ok(())
    }
    pub(in crate::application::terrain_frame::m2) fn is_ready(&self) -> bool {
        !self.active || self.pending.is_finished()
    }
    pub(in crate::application::terrain_frame::m2) fn wait(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.active {
            wait.before_reclaim(&self.pending)?;
        }
        Ok(())
    }
    pub(in crate::application::terrain_frame::m2) fn finish(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !self.active {
            return Ok(());
        }
        let readiness = self.wait(wait);
        let result = self.pending.reclaim(&mut self.jobs);
        self.active = false;
        for job in &mut self.jobs {
            self.calibration.record(&mut job.measurement);
        }
        readiness?;
        result?;
        Ok(())
    }
    pub(in crate::application::terrain_frame::m2) fn release_model(&mut self) {
        debug_assert!(!self.active);
        for job in &mut self.jobs {
            job.release_model();
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn take(
        &mut self,
        index: usize,
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if self.active {
            return Ok(false);
        }
        match self.jobs.first_mut().filter(|job| job.placement() == index) {
            Some(job) => {
                let consumed = job.take(model, clock, view, overrides, output)?;
                #[cfg(test)]
                {
                    self.consumed += usize::from(consumed);
                }
                Ok(consumed)
            }
            None => Ok(false),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn take_samples(
        &mut self,
        index: usize,
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
        output: &mut M2BoneSamples,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if self.active {
            return Ok(false);
        }
        match self.jobs.first_mut().filter(|job| job.placement() == index) {
            Some(job) => {
                let consumed = job.take_samples(model, clock, view, overrides, bones, output)?;
                #[cfg(test)]
                {
                    self.consumed += usize::from(consumed);
                }
                Ok(consumed)
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
impl LatePose {
    pub(in crate::application::terrain_frame::m2) fn consumed(&self) -> usize {
        self.consumed
    }
}
