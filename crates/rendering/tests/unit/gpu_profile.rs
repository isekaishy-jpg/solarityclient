//! Timestamp conversion must preserve short intervals across counter wrap.

use super::TimestampClock;

#[test]
fn timestamp_intervals_handle_queue_width_and_large_epochs() {
    let clock = TimestampClock {
        period_ns: 2.5,
        mask: (1 << 36) - 1,
    };
    assert_eq!(clock.elapsed_ms((1 << 36) - 400_000, 400_000), 2.0);
    assert_eq!(clock.elapsed_ms(1 << 50, (1 << 50) + 800_000), 2.0);
    let full = TimestampClock {
        period_ns: 1.0,
        mask: u64::MAX,
    };
    assert_eq!(full.elapsed_ms(u64::MAX - 499_999, 500_000), 1.0);
    assert_eq!(full.elapsed_ms(u64::MAX - 10, u64::MAX - 9), 0.000001);
}
