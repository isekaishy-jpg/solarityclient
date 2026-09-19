//! One ready group is consumed without a result lock for each scenery placement.

use super::input::{SpatialView, StaticAdmission, StaticAdmissionInput};
use super::job::{MAX_ENTRIES, Results, SpatialJob};
use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_frame::m2::RuntimeTerrainFrameError;
use solarity_cpu::{
    ByteReservation, CostCalibration, CpuBuffer, CpuError, CpuExecutor, CpuStorageClass as Class,
    CpuStorageKind as Kind, FrameBatch, FrameGraphTemplate, FrameJob, FramePriority, JobCost,
};
use std::time::Duration;

/// Current frame inputs never borrow simulation. Only one group's returned
/// values need a consumption buffer; all storage is reused after reclamation.
pub(in crate::application::terrain_frame::m2) struct SpatialBatch {
    jobs: Vec<SpatialJob>,
    job_memory: Option<ByteReservation>,
    indices: CpuBuffer<usize>,
    costs: CpuBuffer<JobCost>,
    handles: CpuBuffer<FrameJob<SpatialJob>>,
    pending: FrameBatch<SpatialJob>,
    current: Results,
    loaded_group: Option<usize>,
    cursor: usize,
    width: usize,
    calibration: CostCalibration,
}

impl Default for SpatialBatch {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            job_memory: None,
            indices: CpuBuffer::default(),
            costs: CpuBuffer::default(),
            handles: CpuBuffer::default(),
            pending: FrameBatch::new(SpatialJob::execute),
            current: std::array::from_fn(|_| None),
            loaded_group: None,
            cursor: 0,
            width: MAX_ENTRIES,
            calibration: CostCalibration::default(),
        }
    }
}

impl SpatialBatch {
    /// Reserves staging and result references before capturing inputs. Refusal
    /// cannot leave workers owning scene state or partially tick any model.
    pub(in crate::application::terrain_frame::m2) fn prepare(
        &mut self,
        cpu: &CpuExecutor,
        maximum: usize,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.finish(&mut FrameWait::Offline)?;
        self.width = self
            .calibration
            .units_for(Duration::from_micros(100))
            .unwrap_or(MAX_ENTRIES)
            .min(MAX_ENTRIES);
        let groups = maximum.div_ceil(self.width);
        self.indices
            .reserve(cpu.storage(), Class::Frame, Kind::Metadata, maximum)?;
        self.costs
            .reserve(cpu.storage(), Class::Frame, Kind::Metadata, groups)?;
        self.handles
            .reserve(cpu.storage(), Class::Frame, Kind::Metadata, groups)?;
        if let Some(memory) = &mut self.job_memory {
            memory.transfer(cpu.storage(), Class::Frame, Kind::Result)?;
        }
        if groups > self.jobs.capacity() {
            let bytes = groups
                .checked_mul(std::mem::size_of::<SpatialJob>())
                .ok_or(CpuError::StorageSizeOverflow)?;
            let mut memory = cpu.storage().reserve(Class::Frame, Kind::Result, bytes)?;
            let mut replacement = Vec::new();
            replacement
                .try_reserve_exact(groups)
                .map_err(|_| CpuError::StorageAllocation)?;
            memory.resize(
                replacement
                    .capacity()
                    .checked_mul(std::mem::size_of::<SpatialJob>())
                    .ok_or(CpuError::StorageSizeOverflow)?,
            )?;
            replacement.append(&mut self.jobs);
            self.jobs = replacement;
            self.job_memory = Some(memory);
        }
        self.indices.clear();
        self.costs.clear();
        self.handles.clear();
        self.cursor = 0;
        self.loaded_group = None;
        Ok(())
    }

    /// Native ordered candidates are grouped without sorting or broad snapshots.
    pub(in crate::application::terrain_frame::m2) fn push(
        &mut self,
        index: usize,
        input: StaticAdmissionInput,
        view: SpatialView,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let group = self.indices.len() / self.width;
        let slot = self.indices.len() % self.width;
        if group == self.jobs.len() {
            self.jobs.push(SpatialJob::new(view));
        }
        let job = &mut self.jobs[group];
        if slot == 0 {
            job.view = view;
            job.first_placement = index;
        }
        job.inputs[slot] = Some(input);
        job.count = slot + 1;
        self.indices.push(index)?;
        Ok(())
    }

    /// Scheduler cells also charge their inline working sets while staging
    /// capacity remains retained. This accounts for the actual peak allocation.
    pub(in crate::application::terrain_frame::m2) fn start(
        &mut self,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.jobs.truncate(self.indices.len().div_ceil(self.width));
        for job in &mut self.jobs {
            job.measurement = self.calibration.prepare(job.count);
            self.costs.push(job.measurement.cost())?;
        }
        if self.jobs.is_empty() {
            return Ok(());
        }
        self.pending.start_costed_graph(
            cpu,
            &FrameGraphTemplate::independent(self.jobs.len())
                .with_priority(FramePriority::Prerequisite),
            &mut self.jobs,
            &[],
            &self.costs,
        )?;
        for group in 0..self.costs.len() {
            self.handles.push(self.pending.job(group)?)?;
        }
        Ok(())
    }

    /// Loads one entire ready group. Domain failures remain in their exact
    /// placement slots rather than escaping ahead of earlier dynamic callbacks.
    pub(in crate::application::terrain_frame::m2) fn is_ready(
        &mut self,
        index: usize,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if self.indices.get(self.cursor).copied() != Some(index) {
            return Ok(true);
        }
        let group = self.cursor / self.width;
        if self.loaded_group == Some(group) {
            return Ok(true);
        }
        if self.pending.outcome(&self.handles[group])?.is_none() {
            return Ok(false);
        }
        self.pending.with_result(&self.handles[group], |job| {
            std::mem::swap(&mut self.current, &mut job.results);
        })?;
        self.loaded_group = Some(group);
        Ok(true)
    }

    /// Every static candidate consumes exactly once, even when rejected. Dynamic
    /// owners have no entry and keep their existing callback-dependent tests.
    pub(in crate::application::terrain_frame::m2) fn take(
        &mut self,
        index: usize,
    ) -> Result<Option<StaticAdmission>, RuntimeTerrainFrameError> {
        if self.indices.get(self.cursor).copied() != Some(index) {
            return Ok(None);
        }
        let slot = self.cursor % self.width;
        self.cursor += 1;
        self.current[slot]
            .take()
            .unwrap_or_else(|| unreachable!("ready static admission is consumed once"))
            .map(Some)
    }

    pub(in crate::application::terrain_frame::m2) fn is_finished(&self) -> bool {
        self.pending.is_finished()
    }

    /// Waits only after the outer driver has exhausted permitted independent work.
    pub(in crate::application::terrain_frame::m2) fn wait_for(
        &self,
        index: usize,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.indices.get(self.cursor).copied() == Some(index) {
            let group = self.cursor / self.width;
            if self.loaded_group != Some(group) {
                wait.before_result(&self.pending, &self.handles[group])?;
            }
        }
        Ok(())
    }

    /// Native completion and exceptional cleanup reclaim every owned group.
    pub(in crate::application::terrain_frame::m2) fn finish(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.pending.close();
        let readiness = wait.before_reclaim(&self.pending);
        let result = self.pending.reclaim(&mut self.jobs);
        for job in &mut self.jobs {
            self.calibration.record(&mut job.measurement);
            for result in &mut job.results {
                *result = None;
            }
        }
        for result in &mut self.current {
            *result = None;
        }
        self.indices.clear();
        self.loaded_group = None;
        readiness?;
        result?;
        Ok(())
    }

    /// Retirement may trail the final result lease return by a worker epilogue.
    pub(in crate::application::terrain_frame::m2) fn wait_finished(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_reclaim(&self.pending)?;
        Ok(())
    }
}
