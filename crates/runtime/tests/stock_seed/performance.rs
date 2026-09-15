//! External frame-rate measurement tests.

// This source inclusion exercises the deterministic clock boundary without
// invoking the production SDL wait entry point from a test process.
#[allow(dead_code)]
#[path = "../../src/performance/limiter.rs"]
mod performance_limiter;
#[path = "../../src/application/performance_overlay/layout.rs"]
mod performance_overlay_layout;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use solarity_runtime::FrameRateCounter;

use performance_limiter::FrameLimiter;
use performance_overlay_layout::{
    FPS_TEXT_REGION_HEIGHT, FPS_TEXT_TOP_LEFT, UI_HEIGHT, overlay_extent,
};

/// The FPS label retains a fixed top-left inset across window resolutions.
#[test]
fn fps_overlay_uses_stock_canvas_top_left_coordinates() {
    assert_eq!(overlay_extent((1024, 768)), [1024.0, UI_HEIGHT]);
    assert_eq!(overlay_extent((1920, 1080)), [1_365.333_4, UI_HEIGHT]);
    assert_eq!(overlay_extent((2560, 1440)), [1_365.333_4, UI_HEIGHT]);
    assert_eq!(FPS_TEXT_TOP_LEFT, [12.0, 10.0]);
    assert_eq!(FPS_TEXT_REGION_HEIGHT, 24.0);
}

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

/// The 1200 FPS idle ceiling retains phase across ordinary scheduler jitter.
#[test]
fn frame_limiter_uses_absolute_deadlines_without_drift() {
    let start = Instant::now();
    let interval = Duration::from_nanos(1_000_000_000 / 1_200);
    let limiter = RefCell::new(FrameLimiter::new());

    let first_delay = run_limiter_wait(&limiter, [start, start + interval]);
    assert_eq!(first_delay, Some(interval));

    let jittered_start = start + interval + Duration::from_micros(200);
    let second_delay = run_limiter_wait(&limiter, [jittered_start, start + interval * 2]);
    assert_eq!(second_delay, Some(interval - Duration::from_micros(200)));
}

/// A complete missed interval starts a fresh schedule instead of catching up.
#[test]
fn frame_limiter_reschedules_after_a_complete_overrun() {
    let start = Instant::now();
    let interval = Duration::from_nanos(1_000_000_000 / 1_200);
    let limiter = RefCell::new(FrameLimiter::new());
    assert_eq!(
        run_limiter_wait(&limiter, [start, start + interval]),
        Some(interval)
    );

    let overrun = start + interval * 4;
    assert_eq!(run_limiter_wait(&limiter, [overrun, overrun]), None);
    assert_eq!(
        run_limiter_wait(
            &limiter,
            [overrun + Duration::from_micros(200), overrun + interval]
        ),
        Some(interval - Duration::from_micros(200))
    );
}

/// A native failure stays observable and cannot silently consume a frame interval.
#[test]
fn frame_limiter_propagates_native_wait_failure_without_advancing_schedule() {
    let start = Instant::now();
    let limiter = RefCell::new(FrameLimiter::new());
    let result = limiter
        .borrow_mut()
        .wait_with(|| start, |_| Err("native wait failed"));
    assert_eq!(result, Err("native wait failed"));
    let interval = Duration::from_nanos(1_000_000_000 / 1_200);
    assert_eq!(
        run_limiter_wait(&limiter, [start, start + interval]),
        Some(interval)
    );
}

/// Runs one deterministic limiter step and captures its requested delay.
fn run_limiter_wait(limiter: &RefCell<FrameLimiter>, instants: [Instant; 2]) -> Option<Duration> {
    let mut instants = VecDeque::from(instants);
    let requested = RefCell::new(None);
    let result: Result<(), std::convert::Infallible> = limiter.borrow_mut().wait_with(
        || instants.pop_front().unwrap_or_else(Instant::now),
        |duration| {
            *requested.borrow_mut() = Some(duration);
            Ok(())
        },
    );
    assert!(result.is_ok());
    requested.into_inner()
}
