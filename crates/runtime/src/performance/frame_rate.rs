//! Fixed-window frame interval measurement.

use std::time::{Duration, Instant};

const DISPLAY_WINDOW: Duration = Duration::from_millis(250);

/// Measures completed-frame intervals over the stock quarter-second window.
pub struct FrameRateCounter {
    window_started: Option<Instant>,
    intervals: u64,
}

impl FrameRateCounter {
    /// Creates an empty counter; the first completion establishes its baseline.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            window_started: None,
            intervals: 0,
        }
    }

    /// Records one completed presentation and publishes only closed windows.
    #[must_use]
    pub fn record(&mut self, completed_at: Instant) -> Option<f64> {
        let Some(started) = self.window_started else {
            self.window_started = Some(completed_at);
            return None;
        };
        self.intervals = self.intervals.saturating_add(1);
        let elapsed = completed_at.saturating_duration_since(started);
        if elapsed < DISPLAY_WINDOW {
            return None;
        }
        let frames_per_second = self.intervals as f64 / elapsed.as_secs_f64();
        self.window_started = Some(completed_at);
        self.intervals = 0;
        Some(frames_per_second)
    }
}

impl Default for FrameRateCounter {
    fn default() -> Self {
        Self::new()
    }
}
