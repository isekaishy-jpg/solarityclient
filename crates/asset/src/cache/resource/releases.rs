//! Pre-registered intrusive indices make last-release delivery allocation-free.

use std::sync::{
    Arc, Mutex, MutexGuard, Weak,
    atomic::{AtomicBool, Ordering},
};

use super::ResourceCacheClock;
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
    released_at_ms: u32,
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
    watchers: Vec<Weak<AtomicBool>>,
}

/// Contains only indices and generations; no payload destructor runs under its lock.
#[derive(Default)]
pub(super) struct Releases {
    state: Mutex<State>,
    clock: Option<ResourceCacheClock>,
}

impl Releases {
    /// Cache mutations and terminal notifications touch metadata only.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|_| unreachable!("release metadata cannot panic"))
    }

    /// Only qualified M2 source caches opt into stock's released-resource age.
    pub(super) fn timed(clock: ResourceCacheClock) -> Self {
        Self {
            state: Mutex::default(),
            clock: Some(clock),
        }
    }

    /// Registry notification is an atomic state change, never a domain callback.
    pub(super) fn subscribe(&self, watcher: &Arc<AtomicBool>) {
        self.lock().watchers.push(Arc::downgrade(watcher));
        watcher.store(true, Ordering::Release);
    }

    /// Signals owner closure without manufacturing a resource release.
    pub(super) fn mark_changed(&self) {
        self.lock().changed();
    }

    /// Collection samples once; every candidate uses the same stock clock observation.
    pub(super) fn collection_time(&self) -> u32 {
        self.clock
            .as_ref()
            .map_or(0, ResourceCacheClock::milliseconds)
    }

    /// Reacquisition withdraws the old grace period and invalidates a late pin destructor.
    pub(super) fn renew(&self, ticket: Ticket) -> Result<Ticket, AssetError> {
        let mut state = self.lock();
        let slot = &state.slots[ticket.index];
        assert!(
            slot.active && slot.generation == ticket.generation,
            "only a registered owner renews a release ticket"
        );
        let generation = slot
            .generation
            .checked_add(1)
            .ok_or(AssetError::IdentityExhausted)?;
        if slot.queued {
            state.unlink(ticket.index);
        }
        state.slots[ticket.index].generation = generation;
        state.slots[ticket.index].released_at_ms = 0;
        state.changed();
        Ok(Ticket {
            index: ticket.index,
            generation,
        })
    }

    /// Only the oldest release is inspected; the registry never scans model payloads.
    pub(super) fn next_delay_ms(&self) -> Option<u32> {
        let state = self.lock();
        let index = state.head?;
        Some(self.delay(self.collection_time(), state.slots[index].released_at_ms))
    }

    /// 81C2BA subtracts wrapping words, then 81C2C2 uses a signed comparison.
    fn delay(&self, now: u32, released_at: u32) -> u32 {
        if self.clock.is_none() {
            return 0;
        }
        let age = now.wrapping_sub(released_at) as i32;
        (10_000i64 - i64::from(age)).max(0) as u32
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
                released_at_ms: 0,
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
                released_at_ms: 0,
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
        // Sample inside the metadata transition so concurrent final releases
        // preserve clock/list order. This clock cannot invoke application code.
        slot.released_at_ms = self.collection_time();
        slot.previous = tail;
        slot.next = None;
        if let Some(tail) = tail {
            state.slots[tail].next = Some(ticket.index);
        } else {
            state.head = Some(ticket.index);
        }
        state.tail = Some(ticket.index);
        state.changed();
    }

    /// Consuming a notification does not revoke the resource's registration.
    pub(super) fn pop(&self, now: u32) -> Option<Ticket> {
        let mut state = self.lock();
        let index = state.head?;
        if self.delay(now, state.slots[index].released_at_ms) != 0 {
            return None;
        }
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
        state.changed();
    }
}

impl State {
    /// Release observers receive correctness state even while profiling is disabled.
    fn changed(&self) {
        for watcher in &self.watchers {
            if let Some(watcher) = watcher.upgrade() {
                watcher.store(true, Ordering::Release);
            }
        }
    }
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
