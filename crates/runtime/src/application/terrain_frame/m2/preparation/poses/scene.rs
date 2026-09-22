//! Callback samples are computed independently; their ordered owner selects exact results.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
mod storage;

use super::input::PoseJob;
use crate::application::frame_pipeline::FrameWait;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuExecutor, CpuStorageBudget, CpuStorageClass as Class,
    CpuStorageKind as Kind, CpuStorageWorkingSet, FrameBatch, FrameGraphTemplate, FrameJob,
    FramePriority,
};
use solarity_rendering::{M2AnimationClock, M2BonePoseOverrides, M2BoneSamples};

pub(in crate::application::terrain_frame::m2) struct ScenePoseExecution<'a, 'window> {
    pub(in crate::application::terrain_frame::m2) cpu: Option<&'a CpuExecutor>,
    pub(in crate::application::terrain_frame::m2) wait: &'a mut FrameWait<'window>,
    pub(in crate::application::terrain_frame::m2) poses: &'a mut ScenePoses,
}

/// Empty worker cells return their owned job to the ordered consumer without copying bones.
pub(in crate::application::terrain_frame::m2) struct ScenePoses {
    cache: CpuBuffer<Option<PoseJob>>,
    jobs: CpuBuffer<Option<PoseJob>>,
    indices: CpuBuffer<Option<usize>>,
    handles: CpuBuffer<FrameJob<Option<PoseJob>>>,
    pending: FrameBatch<Option<PoseJob>>,
    late: FrameBatch<Option<PoseJob>>,
    late_jobs: CpuBuffer<Option<PoseJob>>,
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

/// Scheduler cells and every known pose output share one protected admission.
fn start_outputs(
    cpu: &CpuExecutor,
    pending: &mut FrameBatch<Option<PoseJob>>,
    jobs: &mut CpuBuffer<Option<PoseJob>>,
) -> Result<(), CpuError> {
    let mut outputs = CpuStorageWorkingSet::default();
    for job in jobs.iter().flatten() {
        job.include_output(cpu.storage(), &mut outputs)?;
    }
    pending.start_costed_graph_with_storage(
        cpu,
        &FrameGraphTemplate::independent(jobs.len()).with_priority(FramePriority::Prerequisite),
        jobs,
        &[],
        &[],
        outputs.bytes(),
        |jobs, fund| {
            for job in jobs.iter_mut().flatten() {
                job.admit_output(fund)?;
            }
            Ok(())
        },
    )
}

impl Default for ScenePoses {
    fn default() -> Self {
        Self {
            cache: CpuBuffer::default(),
            jobs: CpuBuffer::default(),
            indices: CpuBuffer::default(),
            handles: CpuBuffer::default(),
            pending: FrameBatch::with_context(execute),
            late: FrameBatch::with_context(execute),
            late_jobs: CpuBuffer::default(),
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
        if self.active
            && let Some(slot) = self.indices.get_mut(index).and_then(Option::take)
        {
            wait.before_result(&self.pending, &self.handles[slot])?;
            self.cache[index] = self
                .pending
                .with_result(&self.handles[slot], Option::take)?;
        }
        if let Some(job) = self.cache.get_mut(index).and_then(Option::as_mut)
            && job.take(&source.model, clock, view, overrides, output)?
        {
            #[cfg(test)]
            {
                self.hits += 1;
                self.palette_jobs += 1;
            }
            return Ok(());
        }
        self.start_late(cpu, index, source, clock, view, overrides, None)?;
        let readiness = wait.before_reclaim(&self.late);
        let result = self.late.reclaim_into(&mut self.late_jobs.writer());
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
        self.seed_job(
            cpu,
            index,
            source,
            clock,
            view,
            overrides,
            Some(bones),
            overrides.bone_sequences.iter().copied(),
        )
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
        self.seed_job(
            cpu,
            index,
            source,
            clock,
            view,
            overrides,
            None,
            overrides.bone_sequences.iter().copied(),
        )
    }

    pub(in crate::application::terrain_frame::m2) fn seeded(&self, index: usize) -> bool {
        self.indices.get(index).is_some_and(Option::is_some)
    }

    /// Playback clocks copy directly into admitted input storage without a temporary Vec.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame::m2) fn seed_sequences(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
        sequences: impl Iterator<Item = (u16, M2AnimationClock)>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.seed_job(
            cpu,
            index,
            source,
            clock,
            view,
            overrides,
            Some(bones),
            sequences,
        )
    }

    pub(in crate::application::terrain_frame::m2) fn start(
        &mut self,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let count = self.jobs.len();
        if count == 0 {
            return Ok(());
        }
        start_outputs(cpu, &mut self.pending, &mut self.jobs)?;
        self.active = true;
        self.handles.clear();
        for index in 0..count {
            self.handles.push(self.pending.job(index)?)?;
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
        if self.active
            && let Some(slot) = self.indices.get_mut(index).and_then(Option::take)
        {
            wait.before_result(&self.pending, &self.handles[slot])?;
            self.cache[index] = self
                .pending
                .with_result(&self.handles[slot], Option::take)?;
        }
        let matching = self
            .cache
            .get(index)
            .and_then(Option::as_ref)
            .is_some_and(|job| job.matches_samples(&source.model, clock, view, overrides, bones));
        if !matching {
            #[cfg(test)]
            {
                self.misses += 1;
            }
            if let Some(cpu) = cpu {
                self.start_late(cpu, index, source, clock, view, overrides, Some(bones))?;
                let readiness = wait.before_reclaim(&self.late);
                let result = self.late.reclaim_into(&mut self.late_jobs.writer());
                self.cache[index] = self.late_jobs.pop().flatten();
                readiness?;
                result?;
            } else {
                let budget = super::storage::budget(cpu)?;
                self.prepare_storage(
                    &budget,
                    index.checked_add(1).ok_or(CpuError::StorageSizeOverflow)?,
                    self.jobs.len(),
                )?;
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
                    overrides.bone_sequences.iter().copied(),
                    &budget,
                )?;
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
        let result = self.pending.reclaim_into(&mut self.jobs.writer());
        self.active = false;
        let late_readiness = wait.before_reclaim(&self.late);
        let late_result = self.late.reclaim_into(&mut self.late_jobs.writer());
        for job in self.jobs.drain().chain(self.late_jobs.drain()).flatten() {
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

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn scene_pose_metadata_refusal_preserves_cached_indices() -> Result<(), CpuError> {
        let bytes = 13 * size_of::<Option<PoseJob>>()
            + 8 * size_of::<Option<usize>>()
            + 4 * size_of::<FrameJob<Option<PoseJob>>>();
        let denied = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes - 1, 0, 0));
        let mut poses = ScenePoses::default();
        assert!(poses.prepare_storage(&denied, 7, 3).is_err());
        assert_eq!(poses.cache.capacity(), 0);
        assert_eq!(poses.jobs.capacity(), 0);
        assert_eq!(poses.late_jobs.capacity(), 0);
        assert_eq!(poses.indices.capacity(), 0);
        assert_eq!(poses.handles.capacity(), 0);
        assert_eq!(denied.snapshot().used(Class::Frame), 0);
        let budget = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes, 0, 0));
        poses.prepare_storage(&budget, 7, 3)?;
        poses.indices[5] = Some(2);
        let addresses = (
            poses.cache.as_ptr(),
            poses.jobs.as_ptr(),
            poses.indices.as_ptr(),
            poses.handles.as_ptr(),
            poses.late_jobs.as_ptr(),
        );
        assert!(poses.prepare_storage(&budget, 9, 3).is_err());
        assert_eq!(poses.cache.len(), 7);
        assert_eq!(poses.indices[5], Some(2));
        poses.prepare_storage(&budget, 7, 3)?;
        assert_eq!(
            addresses,
            (
                poses.cache.as_ptr(),
                poses.jobs.as_ptr(),
                poses.indices.as_ptr(),
                poses.handles.as_ptr(),
                poses.late_jobs.as_ptr()
            )
        );
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        // A retired topology truncates the cache; stale index length must not
        // recreate those removed slots during the next admission.
        poses.cache.truncate(2);
        poses.prepare_storage(&budget, 2, 0)?;
        assert_eq!(poses.cache.len(), 2);
        assert_eq!(poses.indices.len(), 2);
        drop(poses);
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
