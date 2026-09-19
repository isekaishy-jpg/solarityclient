//! Durable completion state retained through the last host access to a fence.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

use ash::vk;

use crate::device::VulkanError;

/// Driver errors and backend panics both release the scoped host-wait owner.
#[derive(Clone, Copy)]
pub(super) enum CompletionFailure {
    Driver(vk::Result),
    Panicked,
}

pub(super) type Outcome = Result<(), CompletionFailure>;

/// Exactly one request may be active. Its caller drains before another begins.
#[derive(Default)]
pub(super) struct State {
    pub(super) request: Option<vk::Fence>,
    pub(super) outcome: Option<Outcome>,
    pub(super) stopping: bool,
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
                return outcome;
            }
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

    /// Consumes the driver outcome only after it has stopped using the fence.
    pub(super) fn finish(&self) -> Result<(), VulkanError> {
        self.shared.drain().map_err(|failure| match failure {
            CompletionFailure::Driver(source) => {
                VulkanError::operation("wait for GPU frame slot", source)
            }
            CompletionFailure::Panicked => {
                VulkanError::operation("wait for GPU frame slot", "completion backend panicked")
            }
        })
    }
}

impl Drop for GpuCompletion<'_> {
    fn drop(&mut self) {
        let _outcome = self.shared.drain();
    }
}
