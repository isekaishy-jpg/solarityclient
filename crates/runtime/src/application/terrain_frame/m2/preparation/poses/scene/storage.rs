//! Scene-pose metadata, copied inputs and outputs use a single protected admission.

use super::*;
use solarity_cpu::{CpuStorageReservation, FrameBatchPlan};

struct Storage<'a> {
    cache: &'a mut CpuBuffer<Option<PoseJob>>,
    indices: &'a mut CpuBuffer<Option<usize>>,
    jobs: &'a mut CpuBuffer<Option<PoseJob>>,
    handles: &'a mut CpuBuffer<FrameJob<Option<PoseJob>>>,
    late_jobs: &'a mut CpuBuffer<Option<PoseJob>>,
}

struct Counts {
    slots: usize,
    cache: usize,
    indices: usize,
    jobs: usize,
    handles: usize,
}

impl Storage<'_> {
    fn include(
        &self,
        budget: &CpuStorageBudget,
        slots: usize,
        jobs: usize,
        plan: &mut CpuStorageWorkingSet,
    ) -> Result<Counts, CpuError> {
        let slots = slots.max(self.cache.len());
        let cache = super::super::storage::capacity(self.cache, slots)?;
        let indices = super::super::storage::capacity(self.indices, slots)?;
        let jobs = super::super::storage::capacity(self.jobs, jobs)?;
        let handles = super::super::storage::capacity(self.handles, jobs)?;
        plan.include(
            self.cache.reservation_bytes(budget, Class::Frame, cache)?,
            self.cache.replacement_credit(cache),
        )?;
        plan.include(
            self.indices
                .reservation_bytes(budget, Class::Frame, indices)?,
            self.indices.replacement_credit(indices),
        )?;
        plan.include(
            self.jobs.reservation_bytes(budget, Class::Frame, jobs)?,
            self.jobs.replacement_credit(jobs),
        )?;
        plan.include(
            self.handles
                .reservation_bytes(budget, Class::Frame, handles)?,
            self.handles.replacement_credit(handles),
        )?;
        plan.include(
            self.late_jobs.reservation_bytes(budget, Class::Frame, 1)?,
            self.late_jobs.replacement_credit(1),
        )?;
        Ok(Counts {
            slots,
            cache,
            indices,
            jobs,
            handles,
        })
    }

    fn reserve(
        &mut self,
        fund: &mut CpuStorageReservation,
        counts: Counts,
    ) -> Result<(), CpuError> {
        self.cache
            .reserve_reserved(fund, Kind::Result, counts.cache)?;
        self.indices
            .reserve_reserved(fund, Kind::Metadata, counts.indices)?;
        self.jobs
            .reserve_reserved(fund, Kind::Result, counts.jobs)?;
        self.handles
            .reserve_reserved(fund, Kind::Metadata, counts.handles)?;
        self.late_jobs.reserve_reserved(fund, Kind::Result, 1)?;
        self.cache.resize_with(counts.slots, || None)?;
        self.indices.resize_with(counts.slots, || None)?;
        Ok(())
    }
}

impl ScenePoses {
    pub(in crate::application::terrain_frame::m2) fn prepare_storage(
        &mut self,
        budget: &CpuStorageBudget,
        slots: usize,
        jobs: usize,
    ) -> Result<(), CpuError> {
        let mut storage = Storage {
            cache: &mut self.cache,
            indices: &mut self.indices,
            jobs: &mut self.jobs,
            handles: &mut self.handles,
            late_jobs: &mut self.late_jobs,
        };
        let mut plan = CpuStorageWorkingSet::default();
        let counts = storage.include(budget, slots, jobs, &mut plan)?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        storage.reserve(&mut fund, counts)
    }

    /// A cache miss preserves the old cached owner if the complete request cannot fit.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_late(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: Option<&[usize]>,
    ) -> Result<(), CpuError> {
        let mut empty = self
            .cache
            .get(index)
            .and_then(Option::as_ref)
            .is_none()
            .then(|| PoseJob::new(source.model.clone()));
        let mut plan = CpuStorageWorkingSet::default();
        // Plan metadata first, matching allocation order, without taking the cached input.
        let mut storage = Storage {
            cache: &mut self.cache,
            indices: &mut self.indices,
            jobs: &mut self.jobs,
            handles: &mut self.handles,
            late_jobs: &mut self.late_jobs,
        };
        let counts = storage.include(
            cpu.storage(),
            index.checked_add(1).ok_or(CpuError::StorageSizeOverflow)?,
            storage.jobs.len(),
            &mut plan,
        )?;
        let job = storage
            .cache
            .get(index)
            .and_then(Option::as_ref)
            .or(empty.as_ref())
            .unwrap_or_else(|| unreachable!("scene pose retains a cached or fresh owner"));
        job.include_preparation(cpu.storage(), source, overrides, bones, &mut plan)?;
        self.late.begin_with_storage(
            cpu,
            FrameBatchPlan::new(1, 0).with_priority(FramePriority::Prerequisite),
            &[],
            plan.bytes(),
            |fund| {
                storage.reserve(fund, counts)?;
                if storage.cache[index].is_none() {
                    storage.cache[index] = empty.take();
                }
                let job = storage.cache[index]
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("funded scene pose owns its cached input"));
                job.prepare_reserved(index, source, clock, view, overrides, fund)?;
                if let Some(bones) = bones {
                    job.request_samples_reserved(bones, fund)?;
                }
                job.admit_output(fund)?;
                storage.late_jobs.push(storage.cache[index].take())?;
                Ok(())
            },
        )?;
        self.late.push_all_with_cost(&mut self.late_jobs, &[])?;
        self.late.close();
        Ok(())
    }
}

impl ScenePoses {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn seed_job(
        &mut self,
        cpu: &CpuExecutor,
        index: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: glam::Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: Option<&[usize]>,
        sequences: impl Iterator<Item = (u16, M2AnimationClock)>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        debug_assert!(!self.active);
        let sequence_count = sequences
            .size_hint()
            .1
            .ok_or(CpuError::StorageSizeOverflow)?;
        let mut empty = self
            .cache
            .get(index)
            .and_then(Option::as_ref)
            .is_none()
            .then(|| PoseJob::new(source.model.clone()));
        let mut storage = Storage {
            cache: &mut self.cache,
            indices: &mut self.indices,
            jobs: &mut self.jobs,
            handles: &mut self.handles,
            late_jobs: &mut self.late_jobs,
        };
        let mut plan = CpuStorageWorkingSet::default();
        let counts = storage.include(
            cpu.storage(),
            index.checked_add(1).ok_or(CpuError::StorageSizeOverflow)?,
            storage
                .jobs
                .len()
                .checked_add(1)
                .ok_or(CpuError::StorageSizeOverflow)?,
            &mut plan,
        )?;
        let job = storage
            .cache
            .get(index)
            .and_then(Option::as_ref)
            .or(empty.as_ref())
            .unwrap_or_else(|| unreachable!("seeded scene pose owns a cached or fresh input"));
        job.include_preparation_counts(
            cpu.storage(),
            source,
            overrides.bone_transforms.len(),
            sequence_count,
            bones,
            &mut plan,
        )?;
        let mut fund = cpu
            .storage()
            .reserve_working_set(Class::Frame, plan.bytes())?;
        storage.reserve(&mut fund, counts)?;
        if storage.cache[index].is_none() {
            storage.cache[index] = empty.take();
        }
        let job = storage.cache[index]
            .as_mut()
            .unwrap_or_else(|| unreachable!("funded scene pose owns its cached input"));
        job.prepare_iter_reserved(
            index,
            source,
            clock,
            view,
            overrides.finger_pose,
            overrides.bone_transforms,
            sequences,
            &mut fund,
        )?;
        if let Some(bones) = bones {
            job.request_samples_reserved(bones, &mut fund)?;
        }
        job.admit_output(&mut fund)?;
        storage.indices[index] = Some(storage.jobs.len());
        storage.jobs.push(storage.cache[index].take())?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../../../../tests/application/scene_pose_admission.rs"]
mod tests;
