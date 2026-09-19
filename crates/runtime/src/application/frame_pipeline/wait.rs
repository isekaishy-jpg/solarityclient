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
    /// Consumes a ready result with one readiness/lease lookup. Only pending
    /// work enters platform service; the closure remains owned until it runs once.
    pub(in crate::application) fn consume<T: Send + 'static, R>(
        &mut self,
        batch: &mut FrameBatch<T>,
        job: &FrameJob<T>,
        consume: impl FnOnce(&mut T) -> R,
    ) -> Result<R, FrameWaitError> {
        if matches!(self, Self::Offline) {
            return Ok(batch.with_result(job, consume)?);
        }
        let mut consume = Some(consume);
        if let Some(result) = batch.try_with_result(job, |input| {
            consume
                .take()
                .unwrap_or_else(|| unreachable!("result consumes once"))(input)
        })? {
            return Ok(result);
        }
        self.before_result(batch, job)?;
        Ok(batch.with_result(
            job,
            consume
                .take()
                .unwrap_or_else(|| unreachable!("pending result retains its consumer")),
        )?)
    }

    pub(in crate::application) fn is_native(&self) -> bool {
        matches!(self, Self::Native(_))
    }

    /// Waits for the exact next consumer, not every later result. Offline callers
    /// keep their existing executor wait at consumption. Terminal domain failures
    /// remain with that consumer rather than changing publication precedence.
    pub(in crate::application) fn before_result<T: Send + 'static>(
        &mut self,
        batch: &FrameBatch<T>,
        job: &FrameJob<T>,
    ) -> Result<(), FrameWaitError> {
        let Self::Native(platform) = self else {
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
