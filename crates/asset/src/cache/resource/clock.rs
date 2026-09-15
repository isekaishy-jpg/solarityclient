//! Wrapping cache milliseconds are independent of gameplay and animation clocks.

use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Instant,
};

/// A non-callback clock can be sampled while updating release metadata.
#[derive(Clone, Debug)]
enum Source {
    Monotonic(Instant),
    Observed(Arc<AtomicU32>),
}

/// Cache time uses signed wrapping elapsed milliseconds, as stock's collector does.
/// A supplied counter supports deterministic replay without invoking domain code
/// under the release lock. Its owner must supply cache time, not pose/frame time.
#[derive(Clone, Debug)]
pub struct ResourceCacheClock(Source);

impl ResourceCacheClock {
    /// Uses one process-wide monotonic origin, with stock's wrapping millisecond width.
    #[must_use]
    pub fn monotonic() -> Self {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        Self(Source::Monotonic(*ORIGIN.get_or_init(Instant::now)))
    }

    /// Observes an explicitly owned wrapping clock for replay or external time integration.
    #[must_use]
    pub fn from_milliseconds(counter: Arc<AtomicU32>) -> Self {
        Self(Source::Observed(counter))
    }

    /// Only a platform counter or atomic read runs inside release bookkeeping.
    pub(super) fn milliseconds(&self) -> u32 {
        match &self.0 {
            Source::Monotonic(origin) => origin.elapsed().as_millis() as u32,
            Source::Observed(counter) => counter.load(Ordering::Acquire),
        }
    }
}
