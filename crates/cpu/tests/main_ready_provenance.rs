//! Main continuation notices retain their request and actual producing phase.

use solarity_cpu::{
    CompletionPort, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStoragePlan, FrameBatch,
    FrameBatchPlan, JobOutcome,
};
use solarity_profiling::{Capture, begin_frame};
use std::{error::Error, io, num::NonZeroUsize};

#[test]
fn main_consumption_links_to_the_phase_that_released_it() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("solarity-main-trace-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=main-provenance".to_owned());
    let (_, path) = capture.toggle()?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let gate = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = gate.producer()?;
    let mut batch = FrameBatch::new(|value: &mut usize| *value += 1);
    let mut ready = cpu.main_ready_queue();
    let frame = begin_frame();
    batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &gate.readiness())?;
    batch.push(&mut Some(41))?;
    batch.close();
    ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(7, &[batch.completion()?])?;
    drop(frame);
    drop(begin_frame());
    assert!(!solarity_profiling::detail_enabled());
    producer.complete(JobOutcome::Succeeded)?;
    ready.wait_until_ready()?;
    // Reclamation waits for the producer's entire publication callback. Readiness
    // itself may be observed before the producer has written its trace record.
    let mut results = Vec::new();
    batch.reclaim(&mut results)?;
    assert_eq!(results, [42]);
    assert_eq!(ready.take_ready().ok_or("main notice")?.key(), 7);
    cpu.shutdown()?;
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
            .ok_or_else(|| io::Error::other(format!("missing main trace operation {label}")))
    };
    let phase = named("cpu.frame.request")?;
    let request = named("cpu.main.request")?;
    let published = named("cpu.main.ready")?;
    let consumed = named("cpu.main.consume")?;
    assert_eq!(published[2], phase[1]);
    assert_eq!(published[3], request[1]);
    assert_eq!(consumed[3], request[1]);
    assert_eq!(published[4], request[4]);
    assert_eq!(consumed[4], request[4]);
    assert_ne!(consumed[4], consumed[5]);
    assert!(consumed[6].parse::<u64>()? >= published[6].parse::<u64>()?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
