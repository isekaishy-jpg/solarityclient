//! Lexical sampled spans use existing profiler clocks wherever possible.

use super::context::next_id;
use super::{TraceContext, TraceGuard};
use std::time::Instant;

/// A synchronous operation with an explicit parent and optional numeric owner.
/// Never retain across await; use a copied TraceContext around individual polls.
#[derive(Default)]
pub struct TraceSpan {
    // None avoids initializing a large record in disabled/unsampled inner probes.
    state: Option<ActiveSpan>,
}

/// Ordinary coarse spans retain only enough provenance for a slow observation.
struct ActiveSpan {
    context: TraceContext,
    parent: u64,
    label: &'static str,
    started: Instant,
    guard: Option<TraceGuard>,
    facts: [u64; 3],
    recording: bool,
}

impl TraceSpan {
    /// Starts owner-specific work only in the selected causal lane.
    #[inline]
    pub fn new(label: &'static str, owner: u64, reason: u64) -> Self {
        let context = TraceContext::capture();
        Self::start(
            label,
            context,
            context.is_sampled().then(Instant::now),
            [owner, reason, 0],
        )
    }

    /// Shares an already-read clock with the enclosing duration profiler.
    #[inline]
    pub(crate) fn start(
        label: &'static str,
        context: TraceContext,
        started: Option<Instant>,
        facts: [u64; 3],
    ) -> Self {
        let Some(started) = started else {
            return Self::default();
        };
        let recording = context.is_sampled();
        if recording {
            super::timestamp(started);
        }
        let child = if recording {
            TraceContext {
                span: next_id(),
                ..context
            }
        } else {
            context
        };
        Self {
            state: Some(ActiveSpan {
                context: child,
                parent: context.span,
                label,
                started,
                guard: recording.then(|| child.enter()),
                facts,
                recording,
            }),
        }
    }

    /// Changes owner metadata without disturbing an already entered parent stack.
    pub(crate) fn annotate(&mut self, owner: u64, reason: u64) -> bool {
        let Some(state) = &mut self.state else {
            return false;
        };
        state.facts = [owner, reason, 0];
        true
    }

    /// Retains ordinary slow work without inventing an unsampled parent chain.
    pub(crate) fn slow(&self, start: Instant, end: Instant) {
        let Some(state) = &self.state else {
            return;
        };
        let duration = nanos(start, end);
        if state.recording || state.context.epoch == 0 || duration < 2_000_000 {
            return;
        }
        let context = TraceContext {
            sampled: true,
            span: next_id(),
            ..state.context
        };
        context.emit(0, 0, "slow", state.label, start, duration, state.facts);
    }

    /// Returns provenance for a queued child or its later publication.
    pub fn context(&self) -> TraceContext {
        self.state
            .as_ref()
            .map_or_else(TraceContext::default, |state| state.context)
    }

    /// Phases overlap nested work and are not additional elapsed cost.
    pub(crate) fn phase(&self, label: &'static str, start: Instant, end: Instant) {
        let Some(state) = &self.state else {
            return;
        };
        if !state.recording {
            return;
        }
        let child = TraceContext {
            span: next_id(),
            ..state.context
        };
        child.emit(
            state.context.span,
            0,
            "phase",
            label,
            start,
            nanos(start, end),
            [0; 3],
        );
    }

    /// Records before restoring the lexical parent; repeated finishing is harmless.
    pub(crate) fn finish(&mut self, now: Instant) {
        if let Some(mut state) = self.state.take() {
            if state.recording {
                state.context.emit(
                    state.parent,
                    0,
                    "span",
                    state.label,
                    state.started,
                    nanos(state.started, now),
                    state.facts,
                );
            }
            state.guard.take();
        }
    }
}

impl Drop for TraceSpan {
    fn drop(&mut self) {
        if self.state.as_ref().is_some_and(|state| state.recording) {
            self.finish(Instant::now());
        }
    }
}

/// Saturates very long diagnostic durations rather than wrapping them.
fn nanos(start: Instant, end: Instant) -> u64 {
    end.saturating_duration_since(start)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}
