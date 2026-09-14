//! Histogram bounds preserve tail evidence without retaining every observation.

use crate::recorder::Aggregate;

/// Holding the writer lock must lose one sample rather than waiting or allocating.
#[test]
fn busy_writer_and_full_event_buffer_report_losses() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("solarity-profile-bounds-{}", std::process::id()));
    let mut capture = crate::Capture::new(&root, "fixture=true".to_owned());
    let (_, path) = capture.toggle()?;
    static SITE: crate::Site = crate::Site::new("fixture.bounds", false);
    SITE.value(1);
    let shard = crate::recorder::registry()
        .lock()
        .map_err(|_| std::io::Error::other("registry unavailable"))?
        .shards[0]
        .clone();
    {
        let data = shard
            .data
            .lock()
            .map_err(|_| std::io::Error::other("shard unavailable"))?;
        SITE.value(2);
        assert_eq!(shard.dropped.load(std::sync::atomic::Ordering::Relaxed), 1);
        drop(data);
    }
    {
        // Freeze only the test writer's registry snapshot. Warm probes must not
        // acquire this lock, and event-capacity assertions need no timing deadline.
        let _registry = crate::recorder::registry()
            .lock()
            .map_err(|_| std::io::Error::other("registry unavailable"))?;
        for _ in 0..crate::recorder::EVENT_CAPACITY + 1 {
            SITE.cpu_duration(crate::generation(), "", std::time::Duration::from_millis(3));
        }
    }
    capture.shutdown()?;
    let metadata = std::fs::read_to_string(path.with_extension("txt"))?;
    assert!(metadata.contains("dropped_samples=1\n"));
    assert!(metadata.contains("dropped_event_rows=1\n"));
    std::fs::remove_dir_all(root)
}

#[test]
fn histogram_bounds_and_overflow_preserve_exact_maximum() {
    for value in [0, 1, 7_999, 8_000, 8_001, 31_001, 5_000_000, u64::MAX] {
        let mut row = Aggregate::default();
        row.record(value);
        assert_eq!(row.maximum, value);
        assert_eq!(row.percentile(95), value);
    }
    let mut first = Aggregate::default();
    let mut second = Aggregate::default();
    for value in 0..1_000 {
        if value % 2 == 0 {
            first.record(value * 9_017);
        } else {
            second.record(value * 9_017);
        }
    }
    first.merge(&second);
    assert_eq!(first.count, 1_000);
    assert!(first.percentile(95) >= 949 * 9_017);
    assert!(first.percentile(95) <= first.maximum);
    first.record(u64::MAX);
    first.record(u64::MAX);
    assert_eq!(first.sum, u64::MAX);
}
