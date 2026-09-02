//! External frame-rate measurement tests.

use std::time::{Duration, Instant};

use solarity_runtime::FrameRateCounter;

/// FPS counts completed intervals and publishes on the quarter-second boundary.
#[test]
fn frame_rate_counter_uses_completed_intervals() -> Result<(), &'static str> {
    let start = Instant::now();
    let mut counter = FrameRateCounter::new();

    assert_eq!(counter.record(start), None);
    for frame in 1..16 {
        assert_eq!(
            counter.record(start + Duration::from_millis(frame * 16)),
            None
        );
    }
    let fps = counter
        .record(start + Duration::from_millis(256))
        .ok_or("quarter-second display window did not close")?;

    assert!((fps - 62.5).abs() < f64::EPSILON);
    Ok(())
}

/// Closing a window starts the next sample from that exact completion.
#[test]
fn frame_rate_counter_resets_at_published_completion() {
    let start = Instant::now();
    let mut counter = FrameRateCounter::new();
    assert_eq!(counter.record(start), None);
    assert_eq!(
        counter.record(start + Duration::from_millis(250)),
        Some(4.0)
    );
    assert_eq!(
        counter.record(start + Duration::from_millis(500)),
        Some(4.0)
    );
}
