//! Demand is distinct from actual output, including early exits and shadow-only work.

use super::Work;

#[test]
fn rejected_shadow_only_and_empty_palette_transactions_remain_distinct() {
    // A non-live generation enables local accounting without starting a global capture.
    let mut work = Work {
        sources: std::collections::HashSet::new(),
        epoch: u64::MAX,
        ..Work::default()
    };
    drop(work.placement());
    {
        let mut shadow = work.placement();
        shadow.admitted = true;
        shadow.environment_shadow = true;
        shadow.palette = true;
        shadow.batch_hit = true;
        shadow.shadow_output = true;
    }
    {
        let mut empty = work.placement();
        empty.admitted = true;
        empty.visible = true;
        empty.palette = true;
        empty.callback_owner = true;
        empty.cpu_output = true;
    }
    assert_eq!(work.visited, 3);
    assert_eq!(work.early_rejected, 1);
    assert_eq!(work.environment_shadow, 1);
    assert_eq!(work.palettes, 2);
    assert_eq!(work.batch_hits, 1);
    assert_eq!(work.shadow_owners, 1);
    assert_eq!(work.palettes_without_draws, 1);
    assert_eq!(work.cpu_outputs, 1);
    assert_eq!(work.mesh_owners, 0);
}

#[test]
fn disabled_placement_accounting_does_not_accumulate() {
    let mut work = Work::default();
    {
        let mut placement = work.placement();
        placement.admitted = true;
        placement.visible = true;
        placement.palette = true;
    }
    assert_eq!(work.visited, 0);
    assert_eq!(work.palettes, 0);
}

#[test]
#[ignore = "manual optimized observer-cost experiment"]
fn benchmark_placement_accounting() {
    use std::hint::black_box;
    use std::time::Instant;
    for epoch in [0, u64::MAX] {
        let start = Instant::now();
        let mut work = Work {
            sources: std::collections::HashSet::new(),
            epoch: black_box(epoch),
            ..Work::default()
        };
        for index in 0..3_000_000 {
            let mut placement = work.placement();
            placement.admitted = black_box(index) % 4 == 0;
            placement.visible = placement.admitted;
            placement.palette = placement.admitted;
            placement.mesh_output = placement.admitted;
            black_box(&placement);
        }
        println!(
            "placement accounting epoch={epoch}: {:.2} ns/visit; visited={}",
            start.elapsed().as_nanos() as f64 / 3_000_000.0,
            black_box(work.visited)
        );
    }
}

#[test]
fn suspended_placement_records_one_transaction_after_publication() {
    let mut work = Work {
        sources: std::collections::HashSet::new(),
        epoch: u64::MAX,
        ..Work::default()
    };
    let mut placement = work.placement();
    placement.admitted = true;
    placement.callback_owner = true;
    placement.palette = true;
    let state = placement.pause();
    assert_eq!(work.visited, 0);
    let mut placement = work.resume_placement(Some(state));
    placement.batch_hit = true;
    placement.cpu_output = true;
    placement.geometry_pending = true;
    drop(placement);
    assert_eq!(work.visited, 1);
    assert_eq!(work.callback_owner, 1);
    assert_eq!(work.batch_hits, 1);
    assert_eq!(work.cpu_outputs, 1);
    assert_eq!(work.palettes_without_draws, 0);
}
