//! Explicit renderer admission with shared CPU execution and caller-owned waits.
use super::VulkanRenderer;
use crate::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadError, BlpTextureUploadRequest,
    WorldFrameExecution,
};
use solarity_asset::BlpTextureSource;
use std::ops::{Deref, DerefMut};

/// A borrowed GPU-admission scope. Authored texture preparation always uses the
/// supplied executor; all other Vulkan operations remain on the calling owner.
pub struct GpuPreparation<'a> {
    renderer: &'a mut VulkanRenderer,
    execution: PreparationExecution<'a>,
}

enum PreparationExecution<'a> {
    Borrowed(&'a mut dyn WorldFrameExecution),
    Offline(&'a solarity_cpu::CpuExecutor),
}
impl PreparationExecution<'_> {
    fn execution(&mut self) -> &mut dyn WorldFrameExecution {
        match self {
            Self::Borrowed(execution) => *execution,
            Self::Offline(cpu) => cpu,
        }
    }
}
impl<'a> GpuPreparation<'a> {
    /// Shared application executor for source work required before GPU admission.
    #[must_use]
    pub fn executor(&mut self) -> &solarity_cpu::CpuExecutor {
        self.execution.execution().executor()
    }

    /// Services native events while an owned source task prepares GPU inputs.
    /// On a native failure or unwind, withdrawal and reclamation precede return.
    /// # Errors
    /// Returns native servicing or CPU completion failures.
    pub fn join_source<T>(
        &mut self,
        task: solarity_cpu::CpuTask<T>,
    ) -> Result<T, crate::VulkanError> {
        crate::WorldRecordingCompletion::join_task(self.execution.execution(), task)
    }

    /// Couples existing renderer and execution owners without allocating a pool.
    pub fn new(
        renderer: &'a mut VulkanRenderer,
        execution: &'a mut dyn WorldFrameExecution,
    ) -> Self {
        Self {
            renderer,
            execution: PreparationExecution::Borrowed(execution),
        }
    }
    /// Uses the supplied executor with blocking waits for offline callers.
    pub fn offline(renderer: &'a mut VulkanRenderer, cpu: &'a solarity_cpu::CpuExecutor) -> Self {
        Self {
            renderer,
            execution: PreparationExecution::Offline(cpu),
        }
    }
    /// Uploads an ordered authored batch through required CPU service workers.
    /// # Errors
    /// Returns admission, decode, native-wait or Vulkan upload errors.
    pub fn upload_blp_textures(
        &mut self,
        requests: &[BlpTextureUploadRequest<'_>],
    ) -> Result<Vec<BlpTextureHandle>, BlpTextureUploadError> {
        self.renderer
            .upload_blp_textures_with_execution(self.execution.execution(), requests)
    }
    /// Uploads one authored source through the same deduplicated worker path.
    /// # Errors
    /// Returns the same failures as batch preparation.
    pub fn upload_blp_texture(
        &mut self,
        source: &BlpTextureSource,
        color: BlpColorSpace,
    ) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        self.renderer
            .upload_blp_texture_with_execution(self.execution.execution(), source, color)
    }
}
impl Deref for GpuPreparation<'_> {
    type Target = VulkanRenderer;
    fn deref(&self) -> &Self::Target {
        self.renderer
    }
}
impl DerefMut for GpuPreparation<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.renderer
    }
}
