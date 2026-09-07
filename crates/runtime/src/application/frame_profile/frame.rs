//! Allocation-free phase sampling for one application transaction.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Retains a fixed phase buffer; disabled profiling makes no clock or cycle calls.
pub(in crate::application) struct RuntimeFrameProfile {
    label: &'static str,
    started: Option<Instant>,
    started_cycles: Option<u64>,
    previous: Duration,
    phases: [(&'static str, Duration); 16],
    count: usize,
}

impl RuntimeFrameProfile {
    /// Enables sampling only for an explicitly instrumented process.
    pub(in crate::application) fn new(label: &'static str) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| std::env::var_os("SOLARITY_FRAME_TIMINGS").is_some());
        Self {
            label,
            started: enabled.then(Instant::now),
            started_cycles: enabled
                .then(crate::platform::current_thread_cycles)
                .flatten(),
            previous: Duration::ZERO,
            phases: [("", Duration::ZERO); 16],
            count: 0,
        }
    }

    /// Attributes elapsed time since the previous mark to this phase.
    pub(in crate::application) fn mark(&mut self, phase: &'static str) {
        let Some(started) = self.started else { return };
        let elapsed = started.elapsed();
        if let Some(slot) = self.phases.get_mut(self.count) {
            *slot = (phase, elapsed.saturating_sub(self.previous));
            self.count += 1;
        }
        self.previous = elapsed;
    }
}

impl Drop for RuntimeFrameProfile {
    fn drop(&mut self) {
        let Some(started) = self.started else { return };
        let now = Instant::now();
        let elapsed = now.duration_since(started);
        let cycles = self
            .started_cycles
            .zip(crate::platform::current_thread_cycles())
            .map(|(start, end)| end.saturating_sub(start));
        super::aggregate::record(
            self.label,
            now,
            elapsed,
            cycles,
            &self.phases[..self.count],
            elapsed.saturating_sub(self.previous),
        );
    }
}
