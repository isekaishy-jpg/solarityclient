//! One persistent driver-wait thread with a single reusable request/result cell.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};

use ash::vk;
use solarity_cpu::CoordinatorNotifier;

use super::state::{CompletionFailure, GpuCompletion, Request, Shared};
use crate::device::VulkanError;

/// Rendering owns this thread and joins it before destroying its Vulkan device.
pub(in crate::device) struct GpuCompletionService {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl GpuCompletionService {
    /// The backend is a host fence wait, owned by this dedicated thread. It may
    /// block; neither scheduler metadata locks nor general CPU jobs are involved.
    pub(in crate::device) fn new(
        mut wait: impl FnMut(vk::Fence) -> Result<(), vk::Result> + Send + 'static,
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
                        catch_unwind(AssertUnwindSafe(|| wait(request.fence)))
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
        self.check_health()?;
        let trace =
            solarity_profiling::TraceContext::capture().fork("rendering.gpu_completion.request");
        {
            let mut state = self.shared.lock();
            state.outcome = None;
            state.request = Some(Request { fence, trace });
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
        completed?;
        Ok(self.check_health()?)
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
