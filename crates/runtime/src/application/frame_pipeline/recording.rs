//! Required GPU command products retain native servicing without moving gameplay publication.

use super::{FrameWait, FrameWaitError};
use solarity_cpu::CpuExecutor;
use solarity_rendering::{VulkanError, WorldFrameExecution, WorldRecordingCompletion};

/// Borrows application execution and native input ownership for one presentation call.
pub(in crate::application) struct RecordingWait<'a, 'platform> {
    cpu: &'a CpuExecutor,
    wait: &'a mut FrameWait<'platform>,
}

impl<'platform> FrameWait<'platform> {
    /// Rendering gets readiness servicing, never access to gameplay or a mutable renderer.
    pub(in crate::application) fn recording<'a>(
        &'a mut self,
        cpu: &'a CpuExecutor,
    ) -> RecordingWait<'a, 'platform> {
        RecordingWait { cpu, wait: self }
    }
}

impl WorldFrameExecution for RecordingWait<'_, '_> {
    fn executor(&self) -> &CpuExecutor {
        self.cpu
    }

    fn wait_for_gpu(
        &mut self,
        completion: &solarity_rendering::GpuCompletion<'_>,
    ) -> Result<(), VulkanError> {
        self.wait.service_gpu(completion)
    }

    fn wait_for_recording(
        &mut self,
        completion: &WorldRecordingCompletion<'_>,
    ) -> Result<(), VulkanError> {
        match self.wait {
            FrameWait::Offline => completion.wait(),
            FrameWait::Native(platform) => platform
                .wait_until_ready(|| Ok::<_, FrameWaitError>(completion.is_ready()))
                .map_err(|error| VulkanError::Operation {
                    operation: "service world recording",
                    message: error.to_string(),
                }),
        }
    }
}
