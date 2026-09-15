//! Measures bounded causal observations before buffer exhaustion, excluding cold setup.

use solarity_profiling::{Capture, TraceContext, begin_frame};
use std::hint::black_box;
use std::io;
use std::time::Instant;

/// Fixed iterations keep sampled output below the trace buffer's capacity.
fn measure(label: &str, iterations: u64, mut operation: impl FnMut()) {
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    println!(
        "{label}: {:.2} ns/call",
        started.elapsed().as_nanos() as f64 / iterations as f64
    );
}

fn main() -> io::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("solarity-causal-overhead"));
    let mut capture = Capture::new(&root, "causal_overhead=true".to_owned());
    for _ in 0..3 {
        measure("disabled scope", 500_000, || {
            black_box(solarity_profiling::profile!("cost.scope"));
        });
        measure("disabled provenance", 500_000, || {
            black_box(TraceContext::capture());
        });
        capture.toggle()?;
        while solarity_profiling::detail_enabled() {
            drop(begin_frame());
        }
        drop(solarity_profiling::profile!("cost.scope"));
        measure("ordinary scope", 500_000, || {
            black_box(solarity_profiling::profile!("cost.scope"));
        });
        measure("ordinary inner owner", 500_000, || {
            let mut span = solarity_profiling::detail_profile!("cost.owner");
            span.trace_owner(1, 2);
            black_box(span);
        });
        while !solarity_profiling::detail_enabled() {
            drop(begin_frame());
        }
        // Warm registration and trace storage outside timing.
        drop(solarity_profiling::profile!("cost.scope"));
        measure("sampled coarse span", 2000, || {
            black_box(solarity_profiling::profile!("cost.scope"));
        });
        measure("sampled owner with phase and output", 2000, || {
            let mut span = solarity_profiling::detail_profile!("cost.owner");
            span.trace_owner(1, 2);
            span.mark("prepare");
            TraceContext::capture().value("cost.output", 1, 0, 6);
            black_box(span);
        });
        capture.shutdown()?;
    }
    Ok(())
}
