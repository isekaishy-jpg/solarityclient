//! Opt-in phase attribution for individual slow application frames.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Keeps profiling allocation-free until a frame exceeds five milliseconds.
pub(super) struct RuntimeFrameProfile {
    label: &'static str,
    started: Option<Instant>,
    started_cycles: Option<u64>,
    previous: Duration,
    phases: [(&'static str, Duration); 16],
    count: usize,
}

impl RuntimeFrameProfile {
    pub(super) fn new(label: &'static str) -> Self {
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

    pub(super) fn mark(&mut self, phase: &'static str) {
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
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(5) {
            let cpu_cycles = self
                .started_cycles
                .zip(crate::platform::current_thread_cycles())
                .map(|(start, end)| end.saturating_sub(start));
            tracing::info!(
                scope = self.label,
                total_ms = elapsed.as_secs_f64() * 1_000.0,
                ?cpu_cycles,
                phases = ?&self.phases[..self.count],
                tail = ?elapsed.saturating_sub(self.previous),
                "profiled slow application frame"
            );
        }
    }
}
