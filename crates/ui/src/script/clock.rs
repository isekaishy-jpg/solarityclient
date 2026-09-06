//! Monotonic client tick source shared by native script queries.

use std::time::Instant;

/// Process-relative millisecond clock exposed to Lua as seconds.
#[derive(Clone, Copy, Debug)]
pub struct UiClientClock {
    started_at: Instant,
    source: Option<fn() -> u32>,
}

impl Default for UiClientClock {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            source: None,
        }
    }
}

impl UiClientClock {
    /// Starts a new client-relative monotonic epoch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses the application's process clock, shared with input and packet time.
    /// The callback returns the low 32 bits of monotonic whole milliseconds.
    #[must_use]
    pub fn from_source(source: fn() -> u32) -> Self {
        Self {
            started_at: Instant::now(),
            source: Some(source),
        }
    }

    /// Returns the low 32 bits of elapsed whole milliseconds.
    ///
    /// The truncation deliberately preserves the stock unsigned tick wrap.
    #[must_use]
    pub fn milliseconds(self) -> u32 {
        self.source.map_or_else(
            || self.started_at.elapsed().as_millis() as u32,
            |source| source(),
        )
    }

    /// Returns the stock Lua projection in millisecond-granularity seconds.
    #[must_use]
    pub fn seconds(self) -> f64 {
        f64::from(self.milliseconds()) * 0.001
    }
}
