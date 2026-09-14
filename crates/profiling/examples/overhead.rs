//! Optimized observer-cost experiment; outputs raw costs without subtracting reports.

use std::hint::black_box;
use std::io;
use std::time::Instant;

use solarity_profiling::{Capture, begin_frame};

const ITERATIONS: u64 = 500_000;

/// Keeps comparable loop work while measuring one static probe policy.
fn measure(label: &str, mut probe: impl FnMut()) {
    let start = Instant::now();
    for index in 0..ITERATIONS {
        black_box(index);
        probe();
    }
    println!(
        "{label}: {:.2} ns/iteration",
        start.elapsed().as_nanos() as f64 / ITERATIONS as f64
    );
}

/// Alternates disabled and ordinary/detail captures; the writer output is inspectable.
fn main() -> io::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("solarity-observer-benchmark"));
    let mut capture = Capture::new(&root, "benchmark=true".to_owned());
    for _ in 0..3 {
        measure("baseline", || {});
        measure("disabled scope", || {
            black_box(solarity_profiling::profile!("bench.scope"));
        });
        measure("disabled value", || {
            solarity_profiling::profile_value!("bench.value", 12);
        });
        measure("disabled cycle span", || {
            black_box(solarity_profiling::profile_cycles!("bench.cycles"));
        });
        capture.toggle()?;
        drop(begin_frame());
        while solarity_profiling::detail_enabled() {
            drop(begin_frame());
        }
        drop(solarity_profiling::profile!("bench.scope"));
        measure("enabled ordinary scope", || {
            black_box(solarity_profiling::profile!("bench.scope"));
        });
        measure("enabled value", || {
            solarity_profiling::profile_value!("bench.value", 12);
        });
        measure("enabled cycle span", || {
            black_box(solarity_profiling::profile_cycles!("bench.cycles"));
        });
        measure("unsampled detail scope", || {
            black_box(solarity_profiling::detail_profile!("bench.detail"));
        });
        while !solarity_profiling::detail_enabled() {
            drop(begin_frame());
        }
        measure("sampled detail scope", || {
            black_box(solarity_profiling::detail_profile!("bench.detail"));
        });
        capture.shutdown()?;
    }
    Ok(())
}
