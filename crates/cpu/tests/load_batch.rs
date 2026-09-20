//! Loading readiness preserves admission, worker eligibility and owned failure state.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass,
    CpuStoragePlan, FrameBatch, FrameBatchPlan, JobOutcome, LoadBatch,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

// Uses real bounded workers; no external resources or application state are required.
fn cpu(workers: usize, capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(workers).ok_or("workers")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(capacity).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// A resource gate retains admission without occupying the only execution lane.
#[test]
fn pending_load_releases_the_only_worker_and_admission_is_shared() -> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(1, 2)?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
    let mut producer = port.producer()?;
    let mut load = LoadBatch::new(CpuService::Required, |value: &mut usize| {
        *value += 1;
        JobOutcome::Succeeded
    });
    let mut jobs = vec![41];
    load.start_after(&cpu, &mut jobs, &[port.readiness()])?;
    assert!(jobs.is_empty());
    assert!(!load.is_finished());
    let permit = cpu.try_reserve()?;
    assert!(matches!(
        cpu.try_reserve(),
        Err(CpuError::AtCapacity { .. })
    ));
    assert_eq!(permit.submit(|| 7).join()?, 7);
    producer.complete(JobOutcome::Succeeded)?;
    load.reclaim(&mut jobs)?;
    assert_eq!(jobs, [42]);
    cpu.shutdown()?;
    Ok(())
}

/// Failure and shutdown both return the original owned input without executing its kernel.
#[test]
fn dependency_failure_and_shutdown_restore_unexecuted_inputs() -> Result<(), Box<dyn Error>> {
    for shutdown in [false, true] {
        let mut cpu = cpu(1, 4)?;
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let mut producer = port.producer()?;
        let mut load = LoadBatch::new(CpuService::Required, |value: &mut usize| {
            *value = 0;
            JobOutcome::Succeeded
        });
        let mut jobs = vec![19];
        load.start_after(&cpu, &mut jobs, &[port.readiness()])?;
        if shutdown {
            cpu.shutdown()?;
        } else {
            producer.complete(JobOutcome::Failed)?;
        }
        assert!(load.reclaim(&mut jobs).is_err());
        assert_eq!(jobs, [19]);
        cpu.shutdown()?;
    }
    Ok(())
}

/// Inherited urgency must never let a blocking loader consume a protected frame worker.
#[test]
fn urgent_frame_dependency_does_not_move_loading_onto_protected_workers()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(2, 4)?;
    let (occupied_tx, occupied_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = cpu.try_submit(move || {
        occupied_tx.send(()).ok();
        release_rx.recv_timeout(Duration::from_secs(10)).ok();
    })?;
    occupied_rx.recv_timeout(Duration::from_secs(5))?;
    let (ran_tx, ran_rx) = mpsc::channel();
    let mut load = LoadBatch::new(
        CpuService::Speculative,
        |sender: &mut mpsc::Sender<String>| {
            sender
                .send(
                    std::thread::current()
                        .name()
                        .unwrap_or("unnamed")
                        .to_owned(),
                )
                .ok();
            JobOutcome::Succeeded
        },
    );
    load.start_after(&cpu, &mut vec![ran_tx], &[])?;
    let control = load.service_control()?;
    let mut frame = FrameBatch::new(|value: &mut usize| *value += 1);
    frame.begin_when(&cpu, FrameBatchPlan::new(1, 0), &load.completion()?)?;
    frame.push(&mut Some(1))?;
    frame.close();
    frame.require_urgent()?;
    // A protected-worker fence observes queued priority propagation without
    // allowing the blocked flexible lane to execute the loading kernel.
    let mut fence = FrameBatch::new(|_: &mut ()| {});
    fence.start(&cpu, &mut vec![()])?;
    fence.reclaim(&mut Vec::new())?;
    let early = ran_rx.try_recv();
    let promoted = control.service();
    release_tx.send(())?;
    blocker.join()?;
    assert!(early.is_err());
    assert_eq!(promoted, CpuService::Required);
    assert!(
        ran_rx
            .recv_timeout(Duration::from_secs(5))?
            .starts_with("solarity-flex-")
    );
    load.reclaim(&mut Vec::new())?;
    let mut results = Vec::new();
    frame.reclaim(&mut results)?;
    assert_eq!(results, [2]);
    cpu.shutdown()?;
    Ok(())
}

/// Independent loading kernels must give queued service a turn between owned inputs.
#[test]
fn each_load_kernel_yields_to_already_queued_service() -> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(1, 4)?;
    let (events_tx, events_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = cpu.try_submit(move || {
        entered_tx.send(()).ok();
        release_rx.recv_timeout(Duration::from_secs(10)).ok();
    })?;
    entered_rx.recv_timeout(Duration::from_secs(5))?;
    let mut load = LoadBatch::new(
        CpuService::Required,
        |value: &mut (usize, mpsc::Sender<usize>)| {
            value.1.send(value.0).ok();
            JobOutcome::Succeeded
        },
    );
    load.start_after(
        &cpu,
        &mut vec![(1, events_tx.clone()), (3, events_tx.clone())],
        &[],
    )?;
    let between = cpu.try_submit(move || events_tx.send(2))?;
    release_tx.send(())?;
    blocker.join()?;
    load.reclaim(&mut Vec::new())?;
    between.join()??;
    assert_eq!(
        [events_rx.recv()?, events_rx.recv()?, events_rx.recv()?],
        [1, 2, 3]
    );
    cpu.shutdown()?;
    Ok(())
}

/// Admission refusal preserves ownership, and service controls cannot cross epoch reuse.
#[test]
fn refused_load_preserves_inputs_and_old_control_cannot_reclassify_new_epoch()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(1, 1)?;
    let occupied = cpu.try_reserve()?;
    let mut load = LoadBatch::new(CpuService::Speculative, |_: &mut usize| {
        JobOutcome::Succeeded
    });
    let mut jobs = vec![5];
    assert!(matches!(
        load.start_after(&cpu, &mut jobs, &[]),
        Err(CpuError::AtCapacity { .. })
    ));
    assert_eq!(jobs, [5]);
    drop(occupied);
    load.start_after(&cpu, &mut jobs, &[])?;
    let old = load.service_control()?;
    load.reclaim(&mut jobs)?;
    load.start_after(&cpu, &mut jobs, &[])?;
    let current = load.service_control()?;
    old.set_service(CpuService::Retirement);
    assert_eq!(current.service(), CpuService::Speculative);
    load.reclaim(&mut jobs)?;
    cpu.shutdown()?;
    Ok(())
}

/// A withdrawn consumer does no derived work and leaves the shared producer usable.
#[test]
fn cancelling_pending_and_queued_loads_preserves_inputs_without_running()
-> Result<(), Box<dyn Error>> {
    for pending in [false, true] {
        let mut cpu = cpu(1, 4)?;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = cpu.try_submit(move || {
            entered_tx.send(()).ok();
            release_rx.recv_timeout(Duration::from_secs(10)).ok();
        })?;
        entered_rx.recv_timeout(Duration::from_secs(5))?;
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let mut producer = port.producer()?;
        let dependencies = if pending {
            vec![port.readiness()]
        } else {
            Vec::new()
        };
        let mut load = LoadBatch::new(CpuService::Required, |value: &mut usize| {
            *value = 0;
            JobOutcome::Succeeded
        });
        let mut jobs = vec![19];
        load.start_after(&cpu, &mut jobs, &dependencies)?;
        load.cancel();
        load.cancel();
        producer.complete(JobOutcome::Succeeded)?;
        release_tx.send(())?;
        blocker.join()?;
        assert!(matches!(
            load.reclaim(&mut jobs),
            Err(CpuError::JobCancelled)
        ));
        assert_eq!(jobs, [19]);
        assert_eq!(port.readiness().outcome()?, Some(JobOutcome::Succeeded));
        cpu.shutdown()?;
    }
    Ok(())
}
