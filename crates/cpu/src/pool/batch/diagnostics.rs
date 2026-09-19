//! Sampled phase drain tails exclude publication and main-thread consumption waits.

use solarity_profiling::{Site, TraceContext};
use std::time::Instant;

/// Only the selected F10 causal frame reads clocks. Retaining the final dispatch
/// and final return identifies the tail after the phase's last kernel starts.
#[derive(Default)]
pub(super) struct DrainTail {
    dispatched: Option<Instant>,
    returned: Option<(Instant, usize)>,
    epoch: u64,
}

impl DrainTail {
    /// Dispatch is serialized by phase metadata, making the final start unambiguous.
    pub fn dispatch(&mut self, trace: TraceContext) {
        if trace.is_sampled() {
            self.dispatched = Some(Instant::now());
            self.epoch = solarity_profiling::generation();
        }
    }

    /// Failure/unwind returns are still part of the tail; cancellation without
    /// execution does not invent kernel time.
    pub fn returned(&mut self, index: usize) {
        if self.dispatched.is_some() {
            self.returned = Some((Instant::now(), index));
        }
    }

    /// Emits once outside scheduler locks. The owner is the final returning job,
    /// whose cpu.frame.execute span carries the frozen estimate and actual time.
    pub fn report(self, trace: TraceContext) {
        let (Some(start), Some((end, index))) = (self.dispatched, self.returned) else {
            return;
        };
        let duration = end.saturating_duration_since(start);
        static TAIL: Site = Site::new("cpu.phase.drain_tail", true);
        TAIL.duration(self.epoch, "", duration);
        trace.value(
            "cpu.phase.last_return",
            index as u64 + 1,
            0,
            duration.as_nanos().min(u128::from(u64::MAX)) as u64,
        );
    }
}
