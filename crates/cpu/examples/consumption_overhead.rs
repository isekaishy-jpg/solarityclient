//! Optimized ready-result observation cost, excluding dispatch and domain work.

use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch};
use solarity_profiling::{Capture, begin_frame, detail_enabled};
use std::{error::Error, hint::black_box, num::NonZeroUsize, time::Instant};

/// Bounded sampled iterations avoid exhausting the causal trace buffer. These
/// modes measure the same public result lease, not a different access baseline.
#[derive(Clone, Copy)]
enum Mode {
    Disabled,
    Ordinary,
    Sampled,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Ordinary => "ordinary",
            Self::Sampled => "sampled",
        }
    }
    fn iterations(self) -> usize {
        match self {
            Self::Sampled => 2_000,
            Self::Disabled | Self::Ordinary => 200_000,
        }
    }
}

/// Warm registration and wait for the producer before measuring consumption.
fn measure(cpu: &CpuExecutor, capture: &mut Capture, mode: Mode) -> Result<(), Box<dyn Error>> {
    if !matches!(mode, Mode::Disabled) {
        capture.toggle()?;
        drop(begin_frame());
        while detail_enabled() != matches!(mode, Mode::Sampled) {
            drop(begin_frame());
        }
    }
    let mut batch = FrameBatch::new(|value: &mut u64| *value += 1);
    let mut values = vec![0];
    batch.start(cpu, &mut values)?;
    let job = batch.job(0)?;
    batch.close();
    batch.wait_until_finished()?;
    for _ in 0..64 {
        black_box(batch.with_result(&job, |value| *value)?);
    }
    let start = Instant::now();
    for _ in 0..mode.iterations() {
        black_box(batch.with_result(&job, |value| {
            *value = black_box(*value).wrapping_add(1);
            *value
        })?);
    }
    let elapsed = start.elapsed();
    batch.reclaim(&mut values)?;
    assert_eq!(values, [mode.iterations() as u64 + 1]);
    println!(
        "{},calls={},total_ns={},ns_per_call={:.2}",
        mode.label(),
        mode.iterations(),
        elapsed.as_nanos(),
        elapsed.as_nanos() as f64 / mode.iterations() as f64
    );
    capture.shutdown()?;
    Ok(())
}

/// Repeated alternating modes expose observer cost and run-to-run variation;
/// results do not represent frame rate, queue wait or a before/after speedup.
fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("solarity-consumption-overhead"));
    let mut capture = Capture::new(&root, "fixture=consumption-overhead".to_owned());
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    for _ in 0..3 {
        for mode in [
            Mode::Disabled,
            Mode::Ordinary,
            Mode::Sampled,
            Mode::Ordinary,
            Mode::Disabled,
        ] {
            measure(&cpu, &mut capture, mode)?;
        }
    }
    cpu.shutdown()?;
    Ok(())
}
