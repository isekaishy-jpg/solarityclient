//! Completion notification is separate from durable typed result ownership.

/// Runtime-owned wake signal for a coordinator that may be parked.
/// Implementations must be thread-safe, bounded and non-panicking. They must
/// retain any native handle for the lifetime of every producer and latch native
/// failures for the coordinator. A notification never carries the only result.
pub trait CoordinatorNotifier: Send + Sync {
    /// Called only after result state is published and scheduler locks released.
    fn notify(&self);
}
