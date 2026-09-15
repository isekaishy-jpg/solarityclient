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
