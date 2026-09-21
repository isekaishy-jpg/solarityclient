//! Ordered capture and failure reclamation at the real CPU/Vulkan ownership boundary.

use super::{MAX_JOBS, PendingScene, SceneCommand, SceneJob, ScenePools, SceneRecording};
use crate::VulkanError;
use crate::device::vulkan_world_frame::recording::{WorldFrameExecution, WorldRecordingCompletion};
use ash::vk;
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch, JobContext,
    JobOutcome,
};
use std::{
    error::Error,
    marker::PhantomData,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// A real shared worker pool with independent required admission and bounded storage.
fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(3, 1, 1, 1)?,
        NonZeroUsize::new(64).ok_or("fixture capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?)
}

/// Partitioning preserves draw/timestamp order, and a later smaller scene has no stale commands.
#[test]
fn capture_preserves_stock_order_across_chunks_and_scene_contraction() -> Result<(), Box<dyn Error>>
{
    let cpu = cpu()?;
    let mut recording = SceneRecording::default();
    for count in [4096, 2, 0, 8192, 128] {
        recording.capture(&cpu, count)?;
        for index in 0..count {
            recording.push(SceneCommand {
                count: index as u32,
                timestamp: (index % 63 == 0).then_some((vk::QueryPool::null(), index as u32)),
                ..Default::default()
            })?;
        }
        let captured = recording
            .jobs
            .iter()
            .flat_map(|job| job.draws.iter())
            .collect::<Vec<_>>();
        assert_eq!(captured.len(), count);
        for (index, draw) in captured.into_iter().enumerate() {
            assert_eq!(draw.count, index as u32);
            assert_eq!(
                draw.timestamp.map(|(_, query)| query),
                (index % 63 == 0).then_some(index as u32)
            );
        }
        assert!(recording.jobs.len() <= cpu.worker_count() * 2);
        assert!(
            recording.spare[recording.jobs.len()..]
                .iter()
                .all(Option::is_none)
        );
    }
    Ok(())
}

static RELEASE: AtomicBool = AtomicBool::new(false);
static FINISHED: AtomicUsize = AtomicUsize::new(0);

/// A controlled recording kernel isolates CPU ownership from driver availability in this test.
fn held_job(job: &mut SceneJob, _: &JobContext<'_>) -> JobOutcome {
    while !RELEASE.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    job.result = Some(Ok(()));
    FINISHED.fetch_or(1 << job.index, Ordering::Release);
    JobOutcome::Succeeded
}

/// Injects the actual main-wait failure boundary, including unwinding during native servicing.
struct InterruptedWait<'a> {
    cpu: &'a CpuExecutor,
    unwind: bool,
}
impl WorldFrameExecution for InterruptedWait<'_> {
    fn executor(&self) -> &CpuExecutor {
        self.cpu
    }
    fn wait_for_gpu(&mut self, completion: &crate::GpuCompletion<'_>) -> Result<(), VulkanError> {
        completion.wait()
    }
    fn wait_for_recording(&mut self, _: &WorldRecordingCompletion<'_>) -> Result<(), VulkanError> {
        RELEASE.store(true, Ordering::Release);
        assert!(!self.unwind, "intentional native servicing unwind");
        Err(VulkanError::WorldFrameCapacity)
    }
}

/// A failed frame cannot reset or destroy pools while any worker still records through them.
#[test]
fn scene_error_and_unwind_reclaim_every_recording_owner() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    for unwind in [false, true] {
        RELEASE.store(false, Ordering::Release);
        FINISHED.store(0, Ordering::Release);
        let mut recording = SceneRecording {
            batch: FrameBatch::with_context(held_job),
            ..Default::default()
        };
        recording.reserve_metadata(&cpu)?;
        recording.jobs.extend((0..4).map(|index| SceneJob {
            index,
            ..Default::default()
        }));
        recording.batch.start(&cpu, &mut recording.jobs)?;
        recording.active = true;
        let pools = ScenePools::default();
        let pending = PendingScene {
            recording: &mut recording,
            _pools: &pools,
            _resources: PhantomData,
            commands: [vk::CommandBuffer::null(); MAX_JOBS],
            count: 4,
        };
        let mut execution = InterruptedWait { cpu: &cpu, unwind };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pending.finish(&mut execution)
        }));
        if unwind {
            assert!(result.is_err());
        } else {
            assert!(matches!(result, Ok(Err(VulkanError::WorldFrameCapacity))));
        }
        assert_eq!(FINISHED.load(Ordering::Acquire), 15);
        assert!(!recording.active);
        assert!(recording.jobs.is_empty());
        for (index, job) in recording.spare[..4].iter().enumerate() {
            assert_eq!(job.as_ref().map(|job| job.index), Some(index));
        }
    }
    Ok(())
}
