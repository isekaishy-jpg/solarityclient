//! Atomic arm/notification protocol, independent of native event semantics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Even generations leave bit zero for the single coordinator's armed flag.
#[derive(Clone, Copy)]
pub(in crate::platform) struct WakeTicket(u64);

/// Only the notification that clears an armed flag needs a native signal.
pub(super) enum SignalAction {
    Coalesced,
    Wake,
    Exhausted,
}

/// Notifications racing with event reset invalidate the observed ticket.
#[derive(Default)]
pub(super) struct WakeSequence(AtomicU64);

impl WakeSequence {
    /// Samples before the coordinator's final readiness/event drain.
    pub(super) fn observe(&self) -> WakeTicket {
        WakeTicket(self.0.load(Ordering::Acquire) & !1)
    }

    /// Arms only if no producer published after the observation.
    pub(super) fn arm(&self, ticket: WakeTicket) -> bool {
        self.0
            .compare_exchange(ticket.0, ticket.0 | 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Advances the durable sequence and consumes an armed flag once.
    pub(super) fn notify(&self) -> SignalAction {
        match self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                state.checked_add(2).map(|next| next & !1)
            }) {
            Ok(state) if state & 1 != 0 => SignalAction::Wake,
            Ok(_) => SignalAction::Coalesced,
            Err(_) => SignalAction::Exhausted,
        }
    }

    /// Disarms after native return, including native failure paths.
    pub(super) fn disarm(&self) {
        self.0.fetch_and(!1, Ordering::AcqRel);
    }
}
