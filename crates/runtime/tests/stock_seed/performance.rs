//! External frame-rate measurement tests.

#[path = "../../src/application/performance_overlay/layout.rs"]
mod performance_overlay_layout;

use std::time::{Duration, Instant};

use solarity_runtime::FrameRateCounter;

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
