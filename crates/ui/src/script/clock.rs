//! Monotonic client tick source shared by native script queries.

use std::time::Instant;

/// Process-relative millisecond clock exposed to Lua as seconds.
#[derive(Clone, Copy, Debug)]
pub struct UiClientClock {
    started_at: Instant,
}

impl Default for UiClientClock {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl UiClientClock {
    /// Starts a new client-relative monotonic epoch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the low 32 bits of elapsed whole milliseconds.
    ///
    /// The truncation deliberately preserves the stock unsigned tick wrap.
    #[must_use]
    pub fn milliseconds(self) -> u32 {
        self.started_at.elapsed().as_millis() as u32
    }

    /// Returns the stock Lua projection in millisecond-granularity seconds.
    #[must_use]
    pub fn seconds(self) -> f64 {
        f64::from(self.milliseconds()) * 0.001
    }
}
