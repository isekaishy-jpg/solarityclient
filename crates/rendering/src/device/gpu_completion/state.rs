//! Durable completion state retained through the last driver access to pinned handles.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

use ash::vk;

use super::{HostOperation, HostOutput};
use crate::device::VulkanError;

/// Driver errors and backend panics both release the scoped host-wait owner.
#[derive(Clone, Copy)]
pub(super) enum CompletionFailure {
    Driver(vk::Result),
    Panicked,
}

pub(super) type Outcome = Result<HostOutput, CompletionFailure>;

/// The single owned request carries its admitting frame to the driver-wait thread.
pub(super) struct Request {
    pub(super) operation: HostOperation,
    pub(super) trace: solarity_profiling::TraceContext,
}

/// Exactly one request may be active. Its caller drains before another begins.
#[derive(Default)]
pub(super) struct State {
    pub(super) operation: Option<HostOperation>,
    pub(super) request: Option<Request>,
    pub(super) outcome: Option<Outcome>,
    pub(super) stopping: bool,
    pub(super) trace: solarity_profiling::TraceContext,
}

/// The mutex protects request/result ownership, never driver or native calls.
#[derive(Default)]
pub(super) struct Shared {
    pub(super) state: Mutex<State>,
    pub(super) changed: Condvar,
    pub(super) ready: AtomicBool,
    pub(super) notifier_failed: AtomicBool,
}

impl Shared {
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Only exceptional early return/unwind parks here; live consumers use the
    /// native coordinator. This also guarantees reset/destroy cannot race a wait.
    pub(super) fn drain(&self) -> Outcome {
        let mut state = self.lock();
        loop {
            if let Some(outcome) = state.outcome {
                let trace = state.trace;
                drop(state);
                // Finish and final release may each observe the same result.
                trace.link("rendering.gpu_completion.observe");
                return outcome;
            }
            state.trace.link("rendering.gpu_completion.drain_need");
            let _origin = state.trace.enter();
            let _profile = solarity_profiling::profile!("rendering.gpu_completion.drain_wait");
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

/// Read-only readiness for one scoped GPU host wait. The renderer remains
/// exclusively borrowed while the native coordinator services this predicate.
/// No Vulkan handle or mutable renderer state crosses this boundary.
pub struct GpuCompletion<'a> {
    pub(super) shared: &'a Shared,
}

impl GpuCompletion<'_> {
    /// Reports terminal publication, including a driver failure. Notification
    /// follows this durable state; consumers do not poll the Vulkan driver.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.shared.ready.load(Ordering::Acquire)
    }

    /// Consumes the driver outcome only after it has stopped using pinned handles.
    pub(super) fn finish(&self) -> Result<HostOutput, VulkanError> {
        let operation = self.shared.lock().operation.unwrap_or_else(|| {
            unreachable!("a scoped GPU completion retains its operation identity")
        });
        self.shared.drain().map_err(|failure| match failure {
            CompletionFailure::Driver(source) => match operation {
                HostOperation::Fence(_) | HostOperation::DeviceIdle => {
                    VulkanError::operation(operation.name(), source)
                }
                HostOperation::Acquire { .. } => {
                    crate::device::vulkan_frame::swapchain_error(operation.name(), source)
                }
            },
            CompletionFailure::Panicked => {
                VulkanError::operation(operation.name(), "completion backend panicked")
            }
        })
    }

    /// Waits synchronously for an explicitly offline renderer. Native callers
    /// service their platform wake bridge using `is_ready` instead.
    ///
    /// # Errors
    /// Returns the original driver or host-backend failure after ownership drains.
    pub fn wait(&self) -> Result<(), VulkanError> {
        self.finish().map(|_| ())
    }
}

impl Drop for GpuCompletion<'_> {
    fn drop(&mut self) {
        let _outcome = self.shared.drain();
    }
}
