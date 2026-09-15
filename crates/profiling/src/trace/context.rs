//! Explicit propagation prevents worker completion time from inventing causality.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::{TraceRecord, timestamp};

thread_local! {
    static CURRENT: Cell<Option<TraceContext>> = const { Cell::new(None) };
}

/// Copyable diagnostic provenance. It owns no task or gameplay resource.
/// Only detail-frame roots produce trace rows; descendants retain that selection.
#[derive(Clone, Copy, Debug, Default)]
pub struct TraceContext {
    pub(crate) epoch: u64,
    pub(crate) frame: u64,
    pub(crate) span: u64,
    pub(crate) sampled: bool,
}

impl TraceContext {
    /// Captures the active parent or starts a root in the current frame's lane.
    /// Disabled calls do not access thread-local storage or a clock.
    #[inline]
    pub fn capture() -> Self {
        let epoch = crate::generation();
        if epoch == 0 {
            return Self::default();
        }
        CURRENT
            .with(|slot| {
                slot.get().map(|context| {
                    if context.epoch == epoch {
                        context
                    } else {
                        Self::default()
                    }
                })
            })
            .unwrap_or(Self {
                epoch,
                frame: crate::scope::frame_number(),
                span: 0,
                sampled: crate::detail_enabled(),
            })
    }

    /// Reports selection in the original capture, rejecting delayed stale work.
    #[inline]
    pub fn is_sampled(self) -> bool {
        self.sampled && self.epoch != 0 && crate::generation() == self.epoch
    }

    /// Enters provenance only for this synchronous call/poll. Never hold across await.
    pub fn enter(self) -> TraceGuard {
        if crate::generation() == 0 {
            return TraceGuard::default();
        }
        let previous = CURRENT.with(|slot| slot.replace(Some(self)));
        TraceGuard {
            previous,
            active: true,
            marker: PhantomData,
        }
    }

    /// Creates a bounded-record logical operation that can cross threads or frames.
    /// It records admission, not an elapsed duration covering parked work.
    pub fn fork(self, label: &'static str) -> Self {
        if !self.is_sampled() {
            return self;
        }
        let child = Self {
            span: next_id(),
            ..self
        };
        child.emit(self.span, 0, "admit", label, Instant::now(), 0, [0; 3]);
        child
    }

    /// Records a dependency from the current consumer to this admitted operation.
    pub fn link(self, label: &'static str) {
        if !self.is_sampled() {
            return;
        }
        let consumer = Self::capture();
        let event = Self {
            span: next_id(),
            ..self
        };
        event.emit(
            consumer.span,
            self.span,
            "link",
            label,
            Instant::now(),
            0,
            [0; 3],
        );
    }

    /// Records numeric owner/reason/workload evidence without strings or registry walks.
    pub fn value(self, label: &'static str, owner: u64, reason: u64, value: u64) {
        if !self.is_sampled() {
            return;
        }
        let event = Self {
            span: next_id(),
            ..self
        };
        event.emit(
            self.span,
            0,
            "value",
            label,
            Instant::now(),
            0,
            [owner, reason, value],
        );
    }

    /// Maps a numeric source to an asset path on a detail frame, once per source.
    /// Callers must pass asset identifiers, never user text or network payloads.
    /// Oversized names are rejected and counted; allocation is restricted to catalog rows.
    pub fn asset(self, source: u64, path: &str) {
        if !self.is_sampled() {
            return;
        }
        if path.len() > 512 {
            crate::recorder::overflow();
            return;
        }
        crate::recorder::record_trace(
            self.epoch,
            TraceRecord {
                id: next_id(),
                parent: self.span,
                related: 0,
                origin_frame: self.frame,
                completion_frame: crate::scope::frame_number(),
                started_ns: timestamp(Instant::now()),
                duration_ns: 0,
                kind: "asset",
                label: "m2.source",
                owner: source,
                reason: 0,
                value: 0,
                name: Some(path.to_owned()),
            },
        );
    }

    /// Associates a retired GPU duration with its submission, without clock alignment.
    pub fn gpu_duration(self, phase: &'static str, duration: std::time::Duration) {
        if !self.is_sampled() {
            return;
        }
        let event = Self {
            span: next_id(),
            ..self
        };
        event.emit(
            self.span,
            0,
            "gpu",
            phase,
            Instant::now(),
            duration.as_nanos().min(u128::from(u64::MAX)) as u64,
            [0; 3],
        );
    }

    /// Attaches an existing CPU measurement to its current operation.
    /// The timestamp is publication time: a duration alone cannot recover its interval.
    pub(crate) fn timing(self, label: &'static str, duration: std::time::Duration) {
        if !self.is_sampled() {
            return;
        }
        let event = Self {
            span: next_id(),
            ..self
        };
        event.emit(
            self.span,
            0,
            "timing",
            label,
            Instant::now(),
            duration.as_nanos().min(u128::from(u64::MAX)) as u64,
            [0; 3],
        );
    }

    /// Writes fixed records through the same nonblocking per-thread storage boundary.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit(
        self,
        parent: u64,
        related: u64,
        kind: &'static str,
        label: &'static str,
        started: Instant,
        duration_ns: u64,
        facts: [u64; 3],
    ) {
        if !self.is_sampled() {
            return;
        }
        crate::recorder::record_trace(
            self.epoch,
            TraceRecord {
                id: self.span,
                parent,
                related,
                origin_frame: self.frame,
                completion_frame: crate::scope::frame_number(),
                started_ns: timestamp(started),
                duration_ns,
                kind,
                label,
                owner: facts[0],
                reason: facts[1],
                value: facts[2],
                name: None,
            },
        );
    }
}

/// IDs are unique across threads/captures; only sampled work increments the counter.
pub(crate) fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Restores lexical thread context on return, error and unwind; cannot change threads.
#[derive(Default)]
pub struct TraceGuard {
    previous: Option<TraceContext>,
    active: bool,
    marker: PhantomData<Rc<()>>,
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        if self.active {
            CURRENT.with(|slot| slot.set(self.previous));
        }
    }
}
