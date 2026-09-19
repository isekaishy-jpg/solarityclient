//! Native readiness waits preserve exact result and gameplay publication order.

use crate::platform::{PlatformError, SdlPlatform};
use solarity_cpu::{CpuError, FrameBatch, FrameJob};
use thiserror::Error;

/// The live client owns SDL on main. Offline replay/fixtures explicitly select
/// the executor's synchronous consumption API because they have no input service.
/// This enum is non-Send through its SDL borrow; it never enters a worker job.
pub(in crate::application) enum FrameWait<'a> {
    Native(&'a mut SdlPlatform),
    Offline,
}

/// A readiness failure does not authorize abandoning worker-owned frame state.
#[derive(Debug, Error)]
pub(in crate::application) enum FrameWaitError {
    #[error(transparent)]
    Cpu(#[from] CpuError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

impl FrameWait<'_> {
    /// Waits for the exact next consumer, not every later result. Offline callers
    /// use the executor condition wait without consuming its payload. Terminal domain failures
    /// remain with that consumer rather than changing publication precedence.
    pub(in crate::application) fn before_result<T: Send + 'static>(
        &mut self,
        batch: &FrameBatch<T>,
        job: &FrameJob<T>,
    ) -> Result<(), FrameWaitError> {
        let Self::Native(platform) = self else {
            batch.wait_for_outcome(job)?;
            return Ok(());
        };
        if batch.outcome(job)?.is_some() {
            return Ok(());
        }
        batch.require_urgent()?;
        let _profile = solarity_profiling::profile!("frame_pipeline.cpu_result_wait");
        platform.wait_until_ready(|| Ok(batch.outcome(job)?.is_some()))
    }

    /// A closed phase is reclaimable only after terminal readiness delivery and
    /// admission release. Callers must still reclaim on a native failure.
    pub(in crate::application) fn before_reclaim<T: Send + 'static>(
        &mut self,
        batch: &FrameBatch<T>,
    ) -> Result<(), FrameWaitError> {
        let Self::Native(platform) = self else {
            batch.wait_until_finished()?;
            return Ok(());
        };
        if batch.is_finished() {
            return Ok(());
        }
        batch.require_urgent()?;
        let _profile = solarity_profiling::profile!("frame_pipeline.cpu_reclaim_wait");
        platform.wait_until_ready(|| Ok(batch.is_finished()))
    }
}
