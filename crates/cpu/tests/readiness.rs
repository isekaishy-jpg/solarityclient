//! External and heterogeneous phase dependencies, cancellation and shutdown.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan, JobOutcome,
};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

fn cpu() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(8).ok_or(CpuError::InvalidJob)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))
}

#[test]
fn external_completion_releases_typed_phases_without_worker_waits() -> Result<(), Box<dyn Error>> {
    let mut cpu = cpu()?;
    let mut port = CompletionPort::new(2, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let token = port.readiness();
    let mut producer = port.producer()?;
    let shared = Arc::new(AtomicUsize::new(0));
    let mut first = FrameBatch::new(|value: &mut Arc<AtomicUsize>| {
        value.store(7, Ordering::Release);
    });
    first.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
    first.push(&mut Some(Arc::clone(&shared)))?;
    first.close();
    let first_ready = first.completion()?;
    let mut second = FrameBatch::new(|value: &mut (Arc<AtomicUsize>, usize)| {
        value.1 = value.0.load(Ordering::Acquire) + 1;
    });
    second.begin_when(&cpu, FrameBatchPlan::new(1, 0), &first_ready)?;
    let result = second.push(&mut Some((Arc::clone(&shared), 0)))?;
    second.close();
    assert_eq!(second.try_with_result(&result, |value| value.1)?, None);
    // The sole worker is free to service an unrelated admitted operation.
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    assert!(producer.complete(JobOutcome::Succeeded)?);
    assert!(!producer.complete(JobOutcome::Succeeded)?);
    assert!(matches!(
        producer.complete(JobOutcome::Failed),
        Err(CpuError::ReadinessConflict)
    ));
    assert_eq!(second.with_result(&result, |value| value.1)?, 8);
    first.reclaim(&mut Vec::new())?;
    second.reclaim(&mut Vec::new())?;
    port.restart(2, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    assert!(matches!(token.outcome(), Err(CpuError::StaleReadiness)));
    assert!(matches!(
        producer.complete(JobOutcome::Succeeded),
        Err(CpuError::StaleReadiness)
    ));
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn removing_one_consumer_releases_its_slot_without_cancelling_the_shared_resource()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let token = port.readiness();
    let mut producer = port.producer()?;
    let mut first = FrameBatch::new(|value| *value += 1);
    first.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
    first.push(&mut Some(1))?;
    let mut next = FrameBatch::new(|value| *value += 1);
    assert!(matches!(
        next.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token),
        Err(CpuError::ReadinessCapacity)
    ));
    drop(first);
    assert_eq!(token.outcome()?, None);
    next.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
    let result = next.push(&mut Some(9))?;
    producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(next.with_result(&result, |value| *value)?, 10);
    next.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn abandoned_producer_fails_dependents_and_returns_owned_inputs() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let producer = port.producer()?;
    let mut batch = FrameBatch::new(|value| *value += 1);
    batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &port.readiness())?;
    let result = batch.push(&mut Some(12))?;
    drop(producer);
    assert!(matches!(
        batch.with_result(&result, |value| *value),
        Err(CpuError::DependencyFailed)
    ));
    let mut inputs = Vec::new();
    assert!(matches!(
        batch.reclaim(&mut inputs),
        Err(CpuError::DependencyFailed)
    ));
    assert_eq!(inputs, [12]);
    Ok(())
}

#[test]
fn shutdown_closes_idle_producers_and_unresolved_gates_without_discarding_inputs()
-> Result<(), Box<dyn Error>> {
    check_shutdown(Shutdown::Explicit)
}

#[test]
fn dropping_executor_closes_idle_producers_and_unresolved_gates_without_discarding_inputs()
-> Result<(), Box<dyn Error>> {
    check_shutdown(Shutdown::Drop)
}

/// Both lifecycle exits must release unowned external gates before drain.
enum Shutdown {
    Explicit,
    Drop,
}

/// The producer remains alive until shutdown finishes; cleanup bounds regressions.
fn check_shutdown(mode: Shutdown) -> Result<(), Box<dyn Error>> {
    let mut cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let mut blocked = FrameBatch::new(|value| *value += 1);
    blocked.begin_when(&cpu, FrameBatchPlan::new(1, 0), &port.readiness())?;
    blocked.push(&mut Some(20))?;
    let mut open = FrameBatch::new(|value| *value += 1);
    open.begin(&cpu, FrameBatchPlan::new(1, 0))?;
    let done = open.push(&mut Some(30))?;
    open.with_result(&done, |_| ())?;
    let (sent, observed) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let result = match mode {
            Shutdown::Explicit => cpu.shutdown(),
            Shutdown::Drop => {
                drop(cpu);
                Ok(())
            }
        };
        let _sent = sent.send(result);
    });
    let shutdown = observed.recv_timeout(Duration::from_secs(5));
    // Closing owners before joining also releases the fixture if this regresses.
    drop(port);
    let mut blocked_inputs = Vec::new();
    let blocked_result = blocked.reclaim(&mut blocked_inputs);
    let mut open_inputs = Vec::new();
    open.reclaim(&mut open_inputs)?;
    thread.join().map_err(|_| "shutdown thread panicked")?;
    shutdown??;
    assert!(matches!(blocked_result, Err(CpuError::DependencyFailed)));
    assert_eq!(blocked_inputs, [20]);
    assert_eq!(open_inputs, [31]);
    Ok(())
}

#[test]
fn already_completed_readiness_can_be_registered_and_reused() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let mut batch = FrameBatch::new(|value| *value += 1);
    for _ in 0..100 {
        port.producer()?.complete(JobOutcome::Succeeded)?;
        batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &port.readiness())?;
        batch.push(&mut Some(7))?;
        let mut output = Vec::new();
        batch.reclaim(&mut output)?;
        assert_eq!(output, [8]);
        port.restart(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    }
    Ok(())
}
