//! Callback samples are computed independently; their ordered owner selects exact results.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use super::input::PoseJob;
use crate::application::frame_pipeline::FrameWait;
use solarity_cpu::{CpuExecutor, FrameBatch, FrameGraphTemplate, FrameJob, FramePriority};
use solarity_rendering::{M2AnimationClock, M2BonePoseOverrides, M2BoneSamples};

pub(in crate::application::terrain_frame::m2) struct ScenePoseExecution<'a, 'window> {
    pub(in crate::application::terrain_frame::m2) cpu: Option<&'a CpuExecutor>,
    pub(in crate::application::terrain_frame::m2) wait: &'a mut FrameWait<'window>,
    pub(in crate::application::terrain_frame::m2) poses: &'a mut ScenePoses,
}

/// Empty worker cells return their owned job to the ordered consumer without copying bones.
pub(in crate::application::terrain_frame::m2) struct ScenePoses {
    cache: Vec<Option<PoseJob>>,
    jobs: Vec<Option<PoseJob>>,
    indices: Vec<Option<usize>>,
    handles: Vec<FrameJob<Option<PoseJob>>>,
    pending: FrameBatch<Option<PoseJob>>,
    late: FrameBatch<Option<PoseJob>>,
    late_jobs: Vec<Option<PoseJob>>,
    active: bool,
    #[cfg(test)]
    pub(in crate::application::terrain_frame::m2) palette_jobs: usize,
    #[cfg(test)]
    pub(in crate::application::terrain_frame::m2) hits: usize,
    #[cfg(test)]
    pub(in crate::application::terrain_frame::m2) misses: usize,
}

fn execute(
    job: &mut Option<PoseJob>,
    context: &solarity_cpu::JobContext<'_>,
) -> solarity_cpu::JobOutcome {
    job.as_mut()
        .unwrap_or_else(|| unreachable!("admitted scene pose owns its inputs"))
        .execute(context)
}

impl Default for ScenePoses {
    fn default() -> Self {
        Self {
            cache: Vec::new(),
            jobs: Vec::new(),
            indices: Vec::new(),
            handles: Vec::new(),
            pending: FrameBatch::with_context(execute),
            late: FrameBatch::with_context(execute),
            late_jobs: Vec::new(),
            active: false,
            #[cfg(test)]
            palette_jobs: 0,
            #[cfg(test)]
            hits: 0,
            #[cfg(test)]
            misses: 0,
        }
    }
}

impl ScenePoses {
    /// Retained scratch follows a live model layout, not an old traversal ordinal.
    pub(in crate::application::terrain_frame::m2) fn retain_layouts(
        &mut self,
        placements: &[super::super::super::M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
    ) {
        debug_assert!(!self.active);
        self.cache.truncate(placements.len());
        for (index, entry) in self.cache.iter_mut().enumerate() {
            if entry.as_ref().is_some_and(|job| {
                sources[placements[index].source_index]
                    .as_ref()
                    .is_none_or(|source| !job.retains_layout(source))
            }) {
                *entry = None;
            }
        }
    }

    /// Vehicle queries retain their full-palette validation and cache boundary.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn palette(
        &mut self,
        cpu: &CpuExecutor,
        wait: &mut FrameWait<'_>,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        output: &mut solarity_rendering::M2BonePose,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.cache.len() <= index {
            self.cache.resize_with(index + 1, || None);
        }
        if self.active
            && let Some(slot) = self.indices.get_mut(index).and_then(Option::take)
        {
            wait.before_result(&self.pending, &self.handles[slot])?;
            self.cache[index] = self
                .pending
                .with_result(&self.handles[slot], Option::take)?;
        }
        if let Some(job) = self.cache[index].as_mut()
            && job.take(&source.model, clock, view, overrides, output)?
        {
            #[cfg(test)]
            {
                self.hits += 1;
                self.palette_jobs += 1;
            }
            return Ok(());
        }
        let mut job = self.cache[index]
            .take()
            .unwrap_or_else(|| PoseJob::new(source.model.clone()));
        job.prepare(
            index,
            source,
            clock,
            view,
            overrides.finger_pose,
            overrides.bone_transforms,
            overrides.bone_sequences.to_vec(),
        );
        job.admit(cpu)?;
        self.late_jobs.push(Some(job));
        self.late.start_graph(
            cpu,
            &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
            &mut self.late_jobs,
            &[],
        )?;
        let readiness = wait.before_reclaim(&self.late);
        let result = self.late.reclaim(&mut self.late_jobs);
        self.cache[index] = self.late_jobs.pop().flatten();
        readiness?;
        result?;
        let job = self.cache[index]
            .as_mut()
            .unwrap_or_else(|| unreachable!("vehicle pose returned"));
        if !job.take(&source.model, clock, view, overrides, output)? {
            return Err(solarity_cpu::CpuError::CompletionLost.into());
        }
        #[cfg(test)]
        {
            self.palette_jobs += 1;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn seed(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.seed_job(cpu, index, source, clock, view, overrides, Some(bones))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn seed_palette(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.seed_job(cpu, index, source, clock, view, overrides, None)
    }

    pub(in crate::application::terrain_frame::m2) fn seeded(&self, index: usize) -> bool {
        self.indices.get(index).is_some_and(Option::is_some)
    }

    #[allow(clippy::too_many_arguments)]
    fn seed_job(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: Option<&[usize]>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        debug_assert!(!self.active);
        if self.cache.len() <= index {
            self.cache.resize_with(index + 1, || None);
        }
        if self.indices.len() <= index {
            self.indices.resize(index + 1, None);
        }
        let mut job = self.cache[index]
            .take()
            .unwrap_or_else(|| PoseJob::new(source.model.clone()));
        job.prepare(
            index,
            source,
            clock,
            view,
            overrides.finger_pose,
            overrides.bone_transforms,
            overrides.bone_sequences.to_vec(),
        );
        if let Some(bones) = bones {
            job.request_samples(bones, cpu.storage())?;
        }
        job.admit(cpu)?;
        self.indices[index] = Some(self.jobs.len());
        self.jobs.push(Some(job));
        Ok(())
    }

    pub(in crate::application::terrain_frame::m2) fn start(
        &mut self,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let count = self.jobs.len();
        if count == 0 {
            return Ok(());
        }
        self.pending.start_costed_graph(
            cpu,
            &FrameGraphTemplate::independent(self.jobs.len())
                .with_priority(FramePriority::Prerequisite),
            &mut self.jobs,
            &[],
            &[],
        )?;
        self.active = true;
        self.handles.clear();
        for index in 0..count {
            self.handles.push(self.pending.job(index)?);
        }
        Ok(())
    }

    pub(in crate::application::terrain_frame::m2) fn is_ready(
        &self,
        index: usize,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let slot = self.indices.get(index).copied().flatten();
        match slot.filter(|_| self.active) {
            Some(slot) => Ok(self.pending.outcome(&self.handles[slot])?.is_some()),
            None => Ok(true),
        }
    }

    pub(in crate::application::terrain_frame::m2) fn wait_for(
        &self,
        index: usize,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.active
            && let Some(slot) = self.indices.get(index).copied().flatten()
        {
            wait.before_result(&self.pending, &self.handles[slot])?;
        }
        Ok(())
    }

    /// A tied callback can change the clock or transform. Only that exact dependency
    /// is resampled; native input remains serviced while its worker owns the numbers.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn sample(
        &mut self,
        cpu: Option<&CpuExecutor>,
        wait: &mut FrameWait<'_>,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
    ) -> Result<&M2BoneSamples, RuntimeTerrainFrameError> {
        if self.cache.len() <= index {
            self.cache.resize_with(index + 1, || None);
        }
        if self.active
            && let Some(slot) = self.indices.get_mut(index).and_then(Option::take)
        {
            wait.before_result(&self.pending, &self.handles[slot])?;
            self.cache[index] = self
                .pending
                .with_result(&self.handles[slot], Option::take)?;
        }
        let matching = self.cache[index]
            .as_ref()
            .is_some_and(|job| job.matches_samples(&source.model, clock, view, overrides, bones));
        if !matching {
            #[cfg(test)]
            {
                self.misses += 1;
            }
            let mut job = self.cache[index]
                .take()
                .unwrap_or_else(|| PoseJob::new(source.model.clone()));
            job.prepare(
                index,
                source,
                clock,
                view,
                overrides.finger_pose,
                overrides.bone_transforms,
                overrides.bone_sequences.to_vec(),
            );
            if let Some(cpu) = cpu {
                job.request_samples(bones, cpu.storage())?;
                job.admit(cpu)?;
                self.late_jobs.push(Some(job));
                self.late.start_costed_graph(
                    cpu,
                    &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
                    &mut self.late_jobs,
                    &[],
                    &[],
                )?;
                let readiness = wait.before_reclaim(&self.late);
                let result = self.late.reclaim(&mut self.late_jobs);
                self.cache[index] = self.late_jobs.pop().flatten();
                readiness?;
                result?;
            } else {
                job.sample_unadmitted(bones);
                self.cache[index] = Some(job);
            }
        } else {
            #[cfg(test)]
            {
                self.hits += 1;
            }
        }
        self.cache[index]
            .as_mut()
            .unwrap_or_else(|| unreachable!("scene pose was returned"))
            .samples()
    }

    pub(in crate::application::terrain_frame::m2) fn finish(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let readiness = wait.before_reclaim(&self.pending);
        let result = self.pending.reclaim(&mut self.jobs);
        self.active = false;
        let late_readiness = wait.before_reclaim(&self.late);
        let late_result = self.late.reclaim(&mut self.late_jobs);
        for job in self
            .jobs
            .drain(..)
            .chain(self.late_jobs.drain(..))
            .flatten()
        {
            let index = job.placement();
            self.cache[index] = Some(job);
        }
        for job in self.cache.iter_mut().flatten() {
            job.release_model();
        }
        self.indices.fill(None);
        readiness?;
        result?;
        late_readiness?;
        late_result?;
        Ok(())
    }
}
