//! Pre-registered intrusive indices make last-release delivery allocation-free.

use std::sync::{Mutex, MutexGuard};

use crate::AssetError;

/// Reuse cannot turn a late final release into another resource's notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Ticket {
    pub(super) index: usize,
    generation: u64,
}

/// The same next link serves the free list only while the slot is unregistered.
struct Slot {
    generation: u64,
    active: bool,
    queued: bool,
    previous: Option<usize>,
    next: Option<usize>,
}

/// One pending record per registered resource, regardless of release/reacquire churn.
#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    free: Option<usize>,
    head: Option<usize>,
    tail: Option<usize>,
}

/// Contains only indices and generations; no payload destructor runs under its lock.
#[derive(Default)]
pub(super) struct Releases(Mutex<State>);

impl Releases {
    /// Cache mutations and terminal notifications touch metadata only.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|_| unreachable!("release metadata cannot panic"))
    }

    /// Allocates only at resource registration, before any consumer owns its ticket.
    pub(super) fn register(&self) -> Result<Ticket, AssetError> {
        let mut state = self.lock();
        let index = if let Some(index) = state.free {
            let generation = state.slots[index]
                .generation
                .checked_add(1)
                .ok_or(AssetError::IdentityExhausted)?;
            state.free = state.slots[index].next;
            state.slots[index] = Slot {
                generation,
                active: true,
                queued: false,
                previous: None,
                next: None,
            };
            index
        } else {
            let index = state.slots.len();
            state.slots.push(Slot {
                generation: 1,
                active: true,
                queued: false,
                previous: None,
                next: None,
            });
            index
        };
        Ok(Ticket {
            index,
            generation: state.slots[index].generation,
        })
    }

    /// A final consumer can only link an existing slot, never allocate a message.
    pub(super) fn notify(&self, ticket: Ticket) {
        let mut state = self.lock();
        let Some(slot) = state.slots.get(ticket.index) else {
            return;
        };
        if !slot.active || slot.generation != ticket.generation || slot.queued {
            return;
        }
        let tail = state.tail;
        let slot = &mut state.slots[ticket.index];
        slot.queued = true;
        slot.previous = tail;
        slot.next = None;
        if let Some(tail) = tail {
            state.slots[tail].next = Some(ticket.index);
        } else {
            state.head = Some(ticket.index);
        }
        state.tail = Some(ticket.index);
    }

    /// Consuming a notification does not revoke the resource's registration.
    pub(super) fn pop(&self) -> Option<Ticket> {
        let mut state = self.lock();
        let index = state.head?;
        let ticket = Ticket {
            index,
            generation: state.slots[index].generation,
        };
        state.unlink(index);
        Some(ticket)
    }

    /// Removes any coalesced notification before allowing this index to be reused.
    pub(super) fn unregister(&self, ticket: Ticket) {
        let mut state = self.lock();
        let slot = &state.slots[ticket.index];
        if !slot.active || slot.generation != ticket.generation {
            return;
        }
        if slot.queued {
            state.unlink(ticket.index);
        }
        let free = state.free;
        let slot = &mut state.slots[ticket.index];
        slot.active = false;
        slot.next = free;
        state.free = Some(ticket.index);
    }
}

impl State {
    /// Unlinks in constant time, including a terminal release racing collection.
    fn unlink(&mut self, index: usize) {
        let previous = self.slots[index].previous;
        let next = self.slots[index].next;
        if let Some(previous) = previous {
            self.slots[previous].next = next;
        } else {
            self.head = next;
        }
        if let Some(next) = next {
            self.slots[next].previous = previous;
        } else {
            self.tail = previous;
        }
        self.slots[index].queued = false;
        self.slots[index].previous = None;
        self.slots[index].next = None;
    }
}
