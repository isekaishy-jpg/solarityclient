//! Ordered command capture fans out recording while main prepares the compositor tail.

use super::super::super::command::RecordContext;
use super::super::execution::{WorldFrameExecution, WorldRecordingCompletion};
use super::command::SceneCommand;
use super::encode::{SceneInheritance, SceneJob};
use crate::VulkanError;
use ash::{Device, vk};
use solarity_cpu::{CpuExecutor, CpuStorageClass, CpuStorageKind, FrameBatch};

/// Bounded parallelism limits command-pool overhead; a large scene grows each range's storage.
pub(in crate::device::vulkan_world_frame) const MAX_JOBS: usize = 32;

/// Slot-owned pools cannot reset until their primary buffer's GPU fence has retired.
pub(in crate::device::vulkan_world_frame) type ScenePools =
    super::super::pools::RecordingPools<34, true>;

/// Compact command allocations and calibrated costs survive ordinary frame reuse.
pub(in crate::device::vulkan_world_frame) struct SceneRecording {
    spare: Vec<Option<SceneJob>>,
    jobs: Vec<SceneJob>,
    metadata: Option<solarity_cpu::ByteReservation>,
    batch: FrameBatch<SceneJob>,
    calibration: solarity_cpu::CostCalibration,
    capture_index: usize,
    chunk_capacity: usize,
    active: bool,
}

impl Default for SceneRecording {
    fn default() -> Self {
        Self {
            spare: Vec::new(),
            jobs: Vec::new(),
            metadata: None,
            batch: FrameBatch::with_context(SceneJob::execute),
            calibration: solarity_cpu::CostCalibration::default(),
            capture_index: 0,
            chunk_capacity: 0,
            active: false,
        }
    }
}

/// This guard joins all workers on error or unwind before releasing registry and pool borrows.
pub(in crate::device::vulkan_world_frame) struct PendingScene<'a> {
    recording: &'a mut SceneRecording,
    _pools: &'a ScenePools,
    _resources: std::marker::PhantomData<&'a ()>,
    commands: [vk::CommandBuffer; MAX_JOBS],
    count: usize,
}

impl SceneRecording {
    /// Reserves the complete compact frame working set before any worker receives resources.
    pub(in crate::device::vulkan_world_frame) fn capture(
        &mut self,
        cpu: &CpuExecutor,
        command_bound: usize,
    ) -> Result<(), VulkanError> {
        debug_assert!(!self.active);
        self.reserve_metadata(cpu)?;
        self.restore();
        let count = command_bound
            .div_ceil(128)
            .min(cpu.worker_count().saturating_mul(2))
            .clamp(1, MAX_JOBS);
        self.chunk_capacity = command_bound.div_ceil(count).max(1);
        self.capture_index = 0;
        for index in 0..count {
            let mut job = self.spare[index].take().unwrap_or_default();
            job.index = index;
            job.draws.clear();
            if let Err(error) = job.draws.reserve(
                cpu.storage(),
                CpuStorageClass::Frame,
                CpuStorageKind::Result,
                self.chunk_capacity,
            ) {
                self.spare[index] = Some(job);
                self.restore();
                return Err(error.into());
            }
            self.jobs.push(job);
        }
        // Retain only the current bounded parallel working set after scene contraction.
        for spare in &mut self.spare[count..] {
            *spare = None;
        }
        Ok(())
    }

    /// Keep the renderer stack small, admitting both retained job arrays before allocation.
    fn reserve_metadata(&mut self, cpu: &CpuExecutor) -> Result<(), VulkanError> {
        if let Some(memory) = &mut self.metadata {
            memory.transfer(
                cpu.storage(),
                CpuStorageClass::Frame,
                CpuStorageKind::Metadata,
            )?;
            return Ok(());
        }
        let bytes = MAX_JOBS * (size_of::<SceneJob>() + size_of::<Option<SceneJob>>());
        let mut memory =
            cpu.storage()
                .reserve(CpuStorageClass::Frame, CpuStorageKind::Metadata, bytes)?;
        let mut spare = Vec::new();
        let mut jobs = Vec::new();
        spare
            .try_reserve_exact(MAX_JOBS)
            .map_err(|_| solarity_cpu::CpuError::StorageAllocation)?;
        jobs.try_reserve_exact(MAX_JOBS)
            .map_err(|_| solarity_cpu::CpuError::StorageAllocation)?;
        memory.resize(
            spare.capacity() * size_of::<Option<SceneJob>>()
                + jobs.capacity() * size_of::<SceneJob>(),
        )?;
        spare.resize_with(MAX_JOBS, || None);
        self.spare = spare;
        self.jobs = jobs;
        self.metadata = Some(memory);
        Ok(())
    }

    /// The stock traversal chooses order; workers cannot reorder this captured command stream.
    pub(in crate::device::vulkan_world_frame) fn push(
        &mut self,
        draw: SceneCommand,
    ) -> Result<(), VulkanError> {
        if self
            .jobs
            .get(self.capture_index)
            .is_some_and(|job| job.draws.len() == self.chunk_capacity)
        {
            self.capture_index += 1;
        }
        self.jobs
            .get_mut(self.capture_index)
            .ok_or(VulkanError::WorldFrameCapacity)?
            .draws
            .push(draw)?;
        Ok(())
    }

    /// Numeric commands are owned; the returned guard keeps their original resources alive.
    pub(in crate::device::vulkan_world_frame) fn begin<'a>(
        &'a mut self,
        cpu: &CpuExecutor,
        context: &'a RecordContext<'_>,
        pools: &'a ScenePools,
        viewport: vk::Viewport,
        scissor: vk::Rect2D,
    ) -> Result<PendingScene<'a>, VulkanError> {
        while self.jobs.last().is_some_and(|job| job.draws.is_empty()) {
            let job = self
                .jobs
                .pop()
                .unwrap_or_else(|| unreachable!("last empty scene job exists"));
            let index = job.index;
            self.spare[index] = Some(job);
        }
        let all_commands = pools.commands();
        let mut commands = [vk::CommandBuffer::null(); MAX_JOBS];
        let mut costs = [solarity_cpu::JobCost::default(); MAX_JOBS];
        for (index, job) in self.jobs.iter_mut().enumerate() {
            job.command = all_commands[index + 2];
            job.device = Some(context.device.clone());
            job.inheritance = Some(SceneInheritance {
                color: context.color_format,
                depth: context.depth_format,
                viewport,
                scissor,
            });
            job.result = None;
            job.measurement = self.calibration.prepare(job.draws.len());
            costs[index] = job.measurement.cost();
            commands[index] = job.command;
        }
        let count = self.jobs.len();
        if count != 0 {
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
        Ok(PendingScene {
            recording: self,
            _pools: pools,
            _resources: std::marker::PhantomData,
            commands,
            count,
        })
    }

    /// Returns allocations to stable chunk owners without retaining draw/resource generations.
    fn restore(&mut self) {
        for mut job in self.jobs.drain(..) {
            let index = job.index;
            job.draws.clear();
            job.device = None;
            self.spare[index] = Some(job);
        }
    }

    /// No error permits command-pool ownership to escape a still-running worker.
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
                        unreachable!("completed scene job has a domain result")
                    }
                });
            }
        }
        self.restore();
        domain?;
        Ok(joined?)
    }
}

impl PendingScene<'_> {
    /// Native input remains serviced at the required join after independent tail recording.
    pub(in crate::device::vulkan_world_frame) fn finish(
        mut self,
        execution: &mut impl WorldFrameExecution,
    ) -> Result<([vk::CommandBuffer; MAX_JOBS], usize), VulkanError> {
        let readiness = if self.recording.active && !self.recording.batch.is_finished() {
            self.recording
                .batch
                .require_urgent()
                .map_err(VulkanError::from)
                .and_then(|()| {
                    let _profile = solarity_profiling::profile!("rendering.scene.pending");
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
        Ok((std::mem::take(&mut self.commands), self.count))
    }
}

impl Drop for PendingScene<'_> {
    fn drop(&mut self) {
        let _ = self.recording.finish();
    }
}

/// Front and tail are small main-owned secondaries while the central ranges use workers.
pub(in crate::device::vulkan_world_frame) fn begin_inline(
    device: &Device,
    command: vk::CommandBuffer,
    color: vk::Format,
    depth: vk::Format,
    viewport: vk::Viewport,
    scissor: vk::Rect2D,
) -> Result<(), VulkanError> {
    super::encode::begin(
        device,
        command,
        SceneInheritance {
            color,
            depth,
            viewport,
            scissor,
        },
    )
}

#[cfg(test)]
#[path = "../../../../../tests/unit/scene_recording.rs"]
mod tests;
