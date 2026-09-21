//! Completion readiness, immutable product ownership and coordinator notification.

mod product;
mod readiness;
mod ready_queue;
pub use product::{ProductOutcome, ProductPublisher, SharedProduct};
pub(crate) use readiness::{Binding, PrioritySink, ReadySink, Subscription};
pub use readiness::{CompletionPort, CompletionProducer, ReadyToken};
pub use ready_queue::{MainReadyQueue, ReadyContinuation};

/// Runtime-owned wake signal for a coordinator that may be parked.
/// Implementations must be thread-safe, bounded and non-panicking. They must
/// retain any native handle for the lifetime of every producer and latch native
/// failures for the coordinator. A notification never carries the only result.
pub trait CoordinatorNotifier: Send + Sync {
    /// Called only after result state is published and scheduler locks released.
    fn notify(&self);
}
