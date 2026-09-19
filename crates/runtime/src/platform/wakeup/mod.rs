//! Coordinator wake sequence, native waiting and SDL watch ownership.

mod sequence;
#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(super) use unsupported::WakeBridge;
#[cfg(windows)]
pub(super) use windows::WakeBridge;

use sequence::{SignalAction, WakeSequence, WakeTicket};

// SDL watches precede queue insertion; recheck this window even without a new
// notification. CPU completion notifications still wake native waits immediately.
pub(super) const MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// A wake is an invitation to inspect durable state, never a completion payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WakeReason {
    Completion,
    Input,
    Deadline,
    Maintenance,
}

#[cfg(test)]
#[path = "../../../tests/platform/wakeup.rs"]
mod tests;
