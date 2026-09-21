//! Recording abandonment joins every exclusive owner before releasing renderer borrows.

use super::super::types::ShadowJob;
use super::super::{
    RecordingPools, ShadowSubmission, WorldFrameExecution, WorldRecordingCompletion,
};
use super::{PendingRecording, ShadowRecording};
use crate::VulkanError;
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch, JobContext,
    JobOutcome,
};
use std::error::Error;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// One fixture owns both gates; neither production kernel nor GPU calls are substituted at runtime.
static RELEASE: AtomicBool = AtomicBool::new(false);
static FINISHED: AtomicUsize = AtomicUsize::new(0);

/// Holds every owner until the main consumption hook fails, then records terminal ownership.
fn held_job(job: &mut ShadowJob, _: &JobContext<'_>) -> JobOutcome {
    while !RELEASE.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    job.result = Some(Ok(()));
    FINISHED.fetch_or(1 << job.pass_index, Ordering::Release);
    JobOutcome::Succeeded
}

/// Forces a deterministic native servicing failure while recording still owns its resources.
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

/// Even an error or unwind before submission must reclaim all four owners in their original slots.
#[test]
fn native_error_and_unwind_join_all_recording_owners() -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(3, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("fixture capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    for unwind in [false, true] {
        RELEASE.store(false, Ordering::Release);
        FINISHED.store(0, Ordering::Release);
        let mut recording = ShadowRecording {
            batch: FrameBatch::with_context(held_job),
            ..Default::default()
        };
        recording.jobs.extend((0..4).map(|pass_index| ShadowJob {
            pass_index,
            ..Default::default()
        }));
        recording.batch.start(&cpu, &mut recording.jobs)?;
        recording.active = true;
        let pools = RecordingPools::default();
        let pending = PendingRecording {
            recording: &mut recording,
            _pools: &pools,
            _resources: PhantomData,
            submission: ShadowSubmission::default(),
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
        for (index, job) in recording.spare.iter().enumerate() {
            assert_eq!(job.as_ref().map(|job| job.pass_index), Some(index));
        }
    }
    Ok(())
}
