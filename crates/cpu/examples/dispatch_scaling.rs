//! Repeatable executor cost/scaling probe; not a live client FPS benchmark.

use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch};
use solarity_profiling::{Capture, begin_frame};
use std::{
    error::Error,
    hint::black_box,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

/// Deterministic independent work, with no clock polling inside a kernel.
struct Input {
    value: u64,
    iterations: usize,
}

/// Retained output verifies that every activation executed exactly once.
fn kernel(input: &mut Input) {
    let mut value = 1_u64;
    for _ in 0..input.iterations {
        value = black_box(value)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
    }
    input.value = input.value.wrapping_add(black_box(value));
}

/// Time only admission, dispatch, completion and reclamation, excluding idle gaps.
fn run(cpu: &CpuExecutor, iterations: usize, idle: bool) -> Result<(), Box<dyn Error>> {
    let mut batch = FrameBatch::new(kernel);
    let mut inputs: Vec<_> = (0..32)
        .map(|_| Input {
            value: 0,
            iterations,
        })
        .collect();
    let samples = if idle { 32 } else { 256 };
    let warmup = 32;
    let mut times = Vec::with_capacity(samples);
    for frame in 0..(samples + warmup) {
        if idle {
            std::thread::sleep(Duration::from_millis(1));
        }
        let _frame = begin_frame();
        let start = Instant::now();
        batch.start(cpu, &mut inputs)?;
        batch.close();
        batch.wait_until_finished()?;
        batch.reclaim(&mut inputs)?;
        if frame >= warmup {
            times.push(start.elapsed().as_nanos() as u64);
        }
    }
    let mut expected = Input {
        value: 0,
        iterations,
    };
    for _ in 0..(samples + warmup) {
        kernel(&mut expected);
    }
    assert!(inputs.iter().all(|input| input.value == expected.value));
    times.sort_unstable();
    println!(
        "workers={},iterations={},idle_gap={},mean_ns={},p50_ns={},p95_ns={},p99_ns={},max_ns={}",
        cpu.worker_count(),
        iterations,
        idle,
        times.iter().sum::<u64>() / times.len() as u64,
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() * 99 / 100],
        times[times.len() - 1]
    );
    Ok(())
}

/// Same 1/3/5/7-worker fixtures repeat off/on/off to expose observer/run variance.
fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("supply an output directory")?;
    let mut capture = Capture::new(&root, "fixture=dispatch-scaling; kernels=32".to_owned());
    for repetition in 0..3 {
        for workers in [1, 3, 5, 7] {
            let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
                CpuExecutionPlan::new(workers - 1, 1, 1, 1)?,
                NonZeroUsize::new(16).ok_or("capacity")?,
                CpuStoragePlan::new(64 << 20, 64 << 20, 0),
            ))?;
            for captured in [false, true, false] {
                if captured {
                    capture.toggle()?;
                }
                println!("repetition={repetition},capture={captured}");
                for iterations in [0, 20_000] {
                    for idle in [false, true] {
                        run(&cpu, iterations, idle)?;
                    }
                }
                if captured {
                    capture.shutdown()?;
                }
            }
            cpu.shutdown()?;
        }
    }
    Ok(())
}
