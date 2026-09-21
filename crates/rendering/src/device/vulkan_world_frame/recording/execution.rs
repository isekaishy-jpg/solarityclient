//! Explicit CPU execution and main-thread servicing at the required submission boundary.

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
    pub(super) batch: &'a dyn RecordingReadiness,
}

impl WorldRecordingCompletion<'_> {
    /// True only after terminal publication and phase admission release.
    pub fn is_ready(&self) -> bool {
        self.batch.is_ready()
    }

    /// Waits without native servicing for an explicitly offline renderer.
    /// # Errors
    /// Returns CPU readiness or invalid worker-consumption errors.
    pub fn wait(&self) -> Result<(), VulkanError> {
        self.batch.wait()
    }
}

/// Erases only readiness, keeping typed job payloads private to their recording owner.
pub(super) trait RecordingReadiness {
    fn is_ready(&self) -> bool;
    fn wait(&self) -> Result<(), VulkanError>;
}
impl<T: Send + 'static> RecordingReadiness for FrameBatch<T> {
    fn is_ready(&self) -> bool {
        self.is_finished()
    }
    fn wait(&self) -> Result<(), VulkanError> {
        Ok(self.wait_until_finished()?)
    }
}

impl<'a> WorldRecordingCompletion<'a> {
    /// Services native readiness while keeping the typed source result private.
    /// Source startup can use this before a renderer exists. Native failure or
    /// unwind requests withdrawal and reclaims the admitted operation.
    /// # Errors
    /// Returns native servicing or CPU completion failures.
    pub fn join_task<T>(
        execution: &mut dyn WorldFrameExecution,
        task: solarity_cpu::CpuTask<T>,
    ) -> Result<T, VulkanError> {
        task.set_service(solarity_cpu::CpuService::Required);
        let pending = SourceCompletion {
            task: std::cell::RefCell::new(Some(task)),
            result: std::cell::RefCell::new(None),
        };
        let waited = execution.wait_for_recording(&WorldRecordingCompletion { batch: &pending });
        if waited.is_err() {
            pending.cancel();
        }
        pending.wait()?;
        let result = pending
            .result
            .take()
            .unwrap_or_else(|| unreachable!("source wait retains its typed result"));
        waited?;
        Ok(result?)
    }

    /// Loading uses the same readiness-only native servicing contract as recording.
    pub(in crate::device) fn loading<T: Send + 'static>(
        batch: &'a solarity_cpu::LoadBatch<T>,
    ) -> Self {
        Self { batch }
    }
}

/// Offline waiting may consume a task channel, but never exposes its payload to
/// the native coordinator. The guard also reclaims on a servicing unwind.
struct SourceCompletion<T> {
    task: std::cell::RefCell<Option<solarity_cpu::CpuTask<T>>>,
    result: std::cell::RefCell<Option<Result<T, solarity_cpu::CpuError>>>,
}
impl<T> SourceCompletion<T> {
    fn cancel(&self) {
        if let Some(task) = self.task.borrow().as_ref() {
            task.cancel();
        }
    }
}
impl<T> RecordingReadiness for SourceCompletion<T> {
    fn is_ready(&self) -> bool {
        self.task
            .borrow()
            .as_ref()
            .is_none_or(|task| task.is_finished())
    }
    fn wait(&self) -> Result<(), VulkanError> {
        if let Some(task) = self.task.take() {
            *self.result.borrow_mut() = Some(task.join());
        }
        Ok(())
    }
}
impl<T> Drop for SourceCompletion<T> {
    fn drop(&mut self) {
        if let Some(task) = self.task.get_mut().take() {
            task.cancel();
            let _ = task.join();
        }
    }
}

impl<T: Send + 'static> RecordingReadiness for solarity_cpu::LoadBatch<T> {
    fn is_ready(&self) -> bool {
        self.is_finished()
    }
    fn wait(&self) -> Result<(), VulkanError> {
        Ok(self.wait_until_finished()?)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/source_completion.rs"]
mod source_tests;
