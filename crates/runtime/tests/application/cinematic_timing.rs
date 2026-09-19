//! Authored/audio-master deadlines are independent of GPU readiness service.

use super::repeated_frame_wait;
use std::time::Duration;

#[test]
fn first_and_advanced_frames_present_immediately() {
    let deadline = Duration::from_millis(40);
    let elapsed = Duration::from_millis(10);
    assert_eq!(repeated_frame_wait(None, 0, deadline, elapsed), None);
    assert_eq!(repeated_frame_wait(Some(0), 1, deadline, elapsed), None);
}

#[test]
fn repeated_frame_waits_only_until_its_authored_deadline() {
    assert_eq!(
        repeated_frame_wait(
            Some(7),
            7,
            Duration::from_millis(75),
            Duration::from_millis(50),
        ),
        Some(Duration::from_millis(25)),
    );
    assert_eq!(
        repeated_frame_wait(
            Some(7),
            7,
            Duration::from_millis(50),
            Duration::from_millis(75),
        ),
        Some(Duration::ZERO),
    );
}
