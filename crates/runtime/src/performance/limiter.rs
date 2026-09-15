//! Absolute-deadline pacing for the interactive presentation loop.

use std::time::{Duration, Instant};

/// Idle presentation ceiling with VSync disabled.
///
/// This remains finite so an empty/minimized scene cannot become an accidental
/// unbounded spin loop, but it leaves enough headroom to expose work that does
/// not fit the 1,200 Hz CPU target.
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

    /// Applies the pacing state machine with explicit clock and delay owners.
    ///
    /// Tests inject monotonic instants through this boundary. Production uses
    /// the native coordinator bridge and propagates native wait failures.
    pub(crate) fn wait_with<E>(
        &mut self,
        mut now: impl FnMut() -> Instant,
        delay: impl FnOnce(Duration) -> Result<(), E>,
    ) -> Result<(), E> {
        let started = now();
        let deadline = *self.deadline.get_or_insert(started + FRAME_INTERVAL);
        if started < deadline {
            delay(deadline - started)?;
        }
        let finished = now();
        // Preserve phase through small scheduler delays. A complete-interval
        // overrun starts a new schedule instead of issuing catch-up frames.
        self.deadline = Some(if finished >= deadline + FRAME_INTERVAL {
            finished + FRAME_INTERVAL
        } else {
            deadline + FRAME_INTERVAL
        });
        Ok(())
    }
}
