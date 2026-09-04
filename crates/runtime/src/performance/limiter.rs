//! Absolute-deadline pacing for the interactive presentation loop.

#![allow(unsafe_code)]

use std::time::{Duration, Instant};

/// Idle presentation ceiling with VSync disabled.
///
/// This remains finite so an empty/minimized scene cannot become an accidental
/// unbounded spin loop, but it leaves enough headroom to expose work that does
/// not fit the 300 Hz performance floor.
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 1_200);

/// Retains one monotonic deadline without accumulating ordinary wait jitter.
pub(crate) struct FrameLimiter {
    deadline: Option<Instant>,
}

impl FrameLimiter {
    /// Creates an unscheduled limiter; the first wait establishes its phase.
    pub(crate) const fn new() -> Self {
        Self { deadline: None }
    }

    /// Waits until the current deadline and advances the absolute schedule.
    pub(crate) fn wait(&mut self) {
        self.wait_with(Instant::now, precise_delay);
    }

    /// Applies the pacing state machine with explicit clock and delay owners.
    ///
    /// Tests inject monotonic instants through this boundary. Production uses
    /// SDL's nanosecond wait, which may busy-wait near the deadline on Windows.
    pub(crate) fn wait_with(
        &mut self,
        mut now: impl FnMut() -> Instant,
        delay: impl FnOnce(Duration),
    ) {
        let started = now();
        let deadline = *self.deadline.get_or_insert(started + FRAME_INTERVAL);
        if started < deadline {
            delay(deadline - started);
        }
        let finished = now();
        // Preserve phase through small scheduler delays. A complete-interval
        // overrun starts a new schedule instead of issuing catch-up frames.
        self.deadline = Some(if finished >= deadline + FRAME_INTERVAL {
            finished + FRAME_INTERVAL
        } else {
            deadline + FRAME_INTERVAL
        });
    }
}

/// Uses the same precise SDL timer primitive as the SolCL reference loop.
fn precise_delay(duration: Duration) {
    let nanoseconds = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
    // SAFETY: SDL_DelayPrecise has no pointer or lifetime preconditions and is
    // documented thread-safe; the process SDL context is live during run().
    unsafe {
        sdl3::sys::timer::SDL_DelayPrecise(nanoseconds);
    }
}
