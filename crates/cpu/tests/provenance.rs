//! Actual queued work keeps its admitting frame through later result consumption.

use std::error::Error;
use std::io;
use std::num::NonZeroUsize;
use std::sync::mpsc;

use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_profiling::{Capture, TraceContext, begin_frame};

#[test]
fn result_consumption_follows_worker_output_and_keeps_request_identity()
-> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("solarity-job-trace-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=cpu-provenance".to_owned());
    let (_, path) = capture.toggle()?;
    let mut executor = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let frame = begin_frame();
    let task = executor.try_submit(move || {
        let _started = started_sender.send(());
        let _released = release_receiver.recv();
        TraceContext::capture().value("fixture.result_ready", 0, 0, 42);
        42
    })?;
    drop(frame);
    started_receiver.recv()?;
    drop(begin_frame());
    assert!(!solarity_profiling::detail_enabled());
    release_sender.send(())?;
    assert_eq!(task.join()?, 42);
    executor.shutdown()?;
    capture.shutdown()?;

    let trace = std::fs::read_to_string(path.with_extension("trace.csv"))?;
    let rows: Vec<Vec<&str>> = trace
        .lines()
        .skip(1)
        .map(|row| row.split(',').collect())
        .collect();
    let named = |label: &str| {
        rows.iter()
            .find(|row| row[9].trim_matches('"') == label)
            .ok_or_else(|| io::Error::other(format!("missing CPU trace operation {label}")))
    };
    let request = named("cpu.job")?;
    let execution = named("cpu.job.execute")?;
    let output = named("fixture.result_ready")?;
    let consumed = named("cpu.job.consume")?;
    assert_eq!(execution[2], request[1]);
    assert_eq!(output[2], execution[1]);
    assert_eq!(consumed[3], request[1]);
    assert_eq!(consumed[4], request[4]);
    assert_ne!(consumed[4], consumed[5]);
    assert!(consumed[6].parse::<u64>()? >= output[6].parse::<u64>()?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
