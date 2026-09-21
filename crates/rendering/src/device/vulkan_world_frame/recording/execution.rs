//! Explicit CPU execution and main-thread servicing at the required submission boundary.

use super::types::ShadowJob;
use crate::VulkanError;
use solarity_cpu::{CpuExecutor, FrameBatch};

/// Supplies the shared CPU executor and the caller's native/offline wait policy.
/// Recording never creates an independent worker pool or moves graphics submission.
pub trait WorldFrameExecution {
    /// Existing application pool used by the required recording phase.
    fn executor(&self) -> &CpuExecutor;

    /// Services platform events while the renderer retains a pending host operation.
    /// The acquired image, semaphore and current camera remain renderer-owned.
    /// # Errors
    /// Returns native or GPU errors; the renderer drains host ownership on failure.
    fn wait_for_gpu(&mut self, completion: &crate::GpuCompletion<'_>) -> Result<(), VulkanError>;

    /// Services the coordinator until recording is ready; payloads remain renderer-owned.
    /// An error still causes unconditional job reclamation before resources are released.
    /// # Errors
    /// Returns native servicing or CPU readiness errors; the renderer still joins all jobs.
    fn wait_for_recording(
        &mut self,
        completion: &WorldRecordingCompletion<'_>,
    ) -> Result<(), VulkanError>;
}

/// Offline rendering explicitly uses synchronous consumption on the supplied shared pool.
impl WorldFrameExecution for &CpuExecutor {
    fn executor(&self) -> &CpuExecutor {
        self
    }
    fn wait_for_gpu(&mut self, completion: &crate::GpuCompletion<'_>) -> Result<(), VulkanError> {
        completion.wait()
    }
    fn wait_for_recording(
        &mut self,
        completion: &WorldRecordingCompletion<'_>,
    ) -> Result<(), VulkanError> {
        completion.wait()
    }
}

/// Readiness-only view; it cannot submit, reset, mutate or steal a recorded command buffer.
pub struct WorldRecordingCompletion<'a> {
    pub(super) batch: &'a FrameBatch<ShadowJob>,
}

impl WorldRecordingCompletion<'_> {
    /// True only after terminal publication and phase admission release.
    pub fn is_ready(&self) -> bool {
        self.batch.is_finished()
    }

    /// Waits without native servicing for an explicitly offline renderer.
    /// # Errors
    /// Returns CPU readiness or invalid worker-consumption errors.
    pub fn wait(&self) -> Result<(), VulkanError> {
        Ok(self.batch.wait_until_finished()?)
    }
}
