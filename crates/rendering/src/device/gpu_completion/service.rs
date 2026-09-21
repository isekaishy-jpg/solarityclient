//! One persistent driver-wait thread with a single reusable request/result cell.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};

use ash::vk;
use solarity_cpu::CoordinatorNotifier;

use super::state::{CompletionFailure, GpuCompletion, Request, Shared};
use super::{HostOperation, HostOutput};
use crate::device::VulkanError;

/// Rendering owns this thread and joins it before destroying its Vulkan device.
pub(in crate::device) struct GpuCompletionService {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl GpuCompletionService {
    /// The backend is a Vulkan host operation owned by this dedicated thread. It may
    /// block; neither scheduler metadata locks nor general CPU jobs are involved.
    pub(in crate::device) fn new(
        mut wait: impl FnMut(HostOperation) -> Result<HostOutput, vk::Result> + Send + 'static,
        notifier: Arc<dyn CoordinatorNotifier>,
    ) -> Result<Self, VulkanError> {
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("solarity-gpu-completion".to_owned())
            .spawn(move || {
                loop {
                    let request = {
                        let mut state = worker.lock();
                        loop {
                            if let Some(request) = state.request.take() {
                                break request;
                            }
                            if state.stopping {
                                return;
                            }
                            state = worker
                                .changed
                                .wait(state)
                                .unwrap_or_else(|error| error.into_inner());
                        }
                    };
                    let _origin = request.trace.enter();
                    let outcome = {
                        let _profile =
                            solarity_profiling::profile!("rendering.gpu_completion.host_wait");
                        catch_unwind(AssertUnwindSafe(|| wait(request.operation)))
                            .map_err(|_| CompletionFailure::Panicked)
                            .and_then(|result| result.map_err(CompletionFailure::Driver))
                    };
                    request.trace.value(
                        "rendering.gpu_completion.host_return",
                        0,
                        0,
                        u64::from(outcome.is_ok()),
                    );
                    // The backend no longer touches this handle after publication.
                    {
                        let mut state = worker.lock();
                        state.outcome = Some(outcome);
                        worker.ready.store(true, Ordering::Release);
                    }
                    worker.changed.notify_all();
                    // A notifier must not panic. Keep terminal state and the
                    // worker alive even if a caller violates that contract.
                    if catch_unwind(AssertUnwindSafe(|| notifier.notify())).is_err() {
                        worker.notifier_failed.store(true, Ordering::Release);
                    }
                }
            })
            .map_err(|source| VulkanError::operation("start GPU completion thread", source))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    /// Surfaces a latched notifier contract violation even on a later ready slot.
    pub(in crate::device) fn check_health(&self) -> Result<(), VulkanError> {
        if self.shared.notifier_failed.load(Ordering::Acquire) {
            return Err(VulkanError::operation(
                "wait for GPU frame slot",
                "coordinator notifier panicked",
            ));
        }
        Ok(())
    }

    /// Drains only unfinished readers of one shared resource. The renderer keeps
    /// all handles pinned for the complete sequence; no temporary fence bank or
    /// CPU worker is needed. Ready readers never dispatch a host wait.
    pub(in crate::device) fn wait_for_pending<E: From<VulkanError>>(
        &mut self,
        fences: impl IntoIterator<Item = vk::Fence>,
        mut is_ready: impl FnMut(vk::Fence) -> Result<bool, VulkanError>,
        mut service_native: impl FnMut(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.check_health()?;
        for fence in fences {
            if !is_ready(fence)? {
                self.wait_for(fence, &mut service_native)?;
            }
        }
        Ok(self.check_health()?)
    }

    /// The device owner must keep the fence live and exclude reset, destruction
    /// and submission throughout this call. `VulkanRenderer`'s exclusive borrow
    /// supplies that invariant. The local completion drains on error and unwind.
    pub(in crate::device) fn wait_for<E: From<VulkanError>>(
        &mut self,
        fence: vk::Fence,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        match self.run(HostOperation::Fence(fence), service_native)? {
            HostOutput::Complete => Ok(()),
            HostOutput::Acquired { .. } => {
                unreachable!("fence operations publish fence completion")
            }
        }
    }

    /// The renderer pins the swapchain and selected unsignaled acquire semaphore.
    /// Native servicing cannot submit, recreate or destroy them while the driver
    /// owns this request. Callback error/unwind still drains host acquisition.
    pub(in crate::device) fn acquire<E: From<VulkanError>>(
        &mut self,
        swapchain: vk::SwapchainKHR,
        semaphore: vk::Semaphore,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(u32, bool), E> {
        match self.run(
            HostOperation::Acquire {
                swapchain,
                semaphore,
            },
            service_native,
        )? {
            HostOutput::Acquired { index, suboptimal } => Ok((index, suboptimal)),
            HostOutput::Complete => unreachable!("image acquisition publishes an acquired image"),
        }
    }

    /// Preserves an existing device-idle lifetime barrier while main services
    /// native events. Exclusive renderer ownership excludes concurrent submission.
    pub(in crate::device) fn idle<E: From<VulkanError>>(
        &mut self,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        match self.run(HostOperation::DeviceIdle, service_native)? {
            HostOutput::Complete => Ok(()),
            HostOutput::Acquired { .. } => {
                unreachable!("device retirement does not acquire images")
            }
        }
    }

    /// One zero-timeout probe avoids dispatch when an image is already available.
    /// Vulkan WSI guarantees NOT_READY leaves the semaphore/fence unaffected;
    /// only that result transfers acquisition to the host thread. No polling loop
    /// or additional image is introduced, and other driver failures stay terminal.
    pub(in crate::device) fn acquire_when_pending<E: From<VulkanError>>(
        &mut self,
        swapchain: vk::SwapchainKHR,
        semaphore: vk::Semaphore,
        probe: impl FnOnce() -> Result<(u32, bool), vk::Result>,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(u32, bool), E> {
        self.check_health()?;
        match probe() {
            Ok(acquired) => Ok(acquired),
            Err(vk::Result::NOT_READY) => self.acquire(swapchain, semaphore, service_native),
            Err(error) => Err(crate::device::vulkan_frame::swapchain_error(
                "acquire frame image",
                error,
            )
            .into()),
        }
    }

    /// One reusable request/result cell carries the selected typed host operation.
    fn run<E: From<VulkanError>>(
        &mut self,
        operation: HostOperation,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<HostOutput, E> {
        self.check_health()?;
        let trace =
            solarity_profiling::TraceContext::capture().fork("rendering.gpu_completion.request");
        {
            let mut state = self.shared.lock();
            state.outcome = None;
            state.request = Some(Request { operation, trace });
            state.operation = Some(operation);
            state.trace = trace;
            self.shared.ready.store(false, Ordering::Release);
        }
        self.shared.changed.notify_one();
        let completion = GpuCompletion {
            shared: &self.shared,
        };
        let serviced = service_native(&completion);
        let completed = completion.finish();
        serviced?;
        let completed = completed?;
        self.check_health()?;
        Ok(completed)
    }
}

impl Drop for GpuCompletionService {
    fn drop(&mut self) {
        self.shared.lock().stopping = true;
        self.shared.changed.notify_one();
        if let Some(thread) = self.thread.take() {
            let _joined = thread.join();
        }
    }
}
