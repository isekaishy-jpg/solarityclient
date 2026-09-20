//! Explicit worker/service eligibility preserves frame progress under bulk load.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

/// Impossible plans fail before worker creation or admission can begin.
#[test]
fn invalid_execution_counts_are_rejected() {
    for (protected, flexible, reserve, bulk) in [
        (0, 0, 0, 0),
        (2, 1, 0, 1),
        (2, 1, 1, 0),
        (2, 1, 2, 1),
        (2, 1, 1, 2),
        (usize::MAX, 1, 1, 1),
    ] {
        assert!(matches!(
            CpuExecutionPlan::new(protected, flexible, reserve, bulk),
            Err(CpuError::InvalidExecutionPlan)
        ));
    }
}

/// With no protected worker, capped bulk service still leaves an explicitly
/// configured flexible worker able to complete frames while callers are blocked.
#[test]
fn bulk_limit_leaves_remaining_flexible_capacity_for_frames() -> Result<(), Box<dyn Error>> {
    exercise_plan(CpuExecutionPlan::new(0, 3, 1, 2)?)
}

/// More than one reserved service worker is real capacity, not merely reported
/// configuration. Protected workers still complete frames under those calls.
#[test]
fn multiple_service_workers_obey_the_resolved_plan() -> Result<(), Box<dyn Error>> {
    exercise_plan(CpuExecutionPlan::new(2, 3, 2, 3)?)
}

/// The timeout bounds a broken scheduler; no elapsed time is a performance claim.
fn exercise_plan(plan: CpuExecutionPlan) -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        plan,
        NonZeroUsize::new(16).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?;
    assert_eq!(cpu.execution_plan(), plan);
    assert_eq!(cpu.worker_count(), plan.worker_count().get());
    assert_eq!(cpu.background_worker_count(), plan.flexible_workers().get());
    assert_eq!(cpu.frame_worker_count(), plan.protected_workers());
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (started, observed) = mpsc::channel();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let gate = Arc::clone(&gate);
        let active = Arc::clone(&active);
        let peak = Arc::clone(&peak);
        let started = started.clone();
        tasks.push(cpu.try_submit(move || {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            let _sent = started.send(());
            let mut released = gate
                .0
                .lock()
                .unwrap_or_else(|_| unreachable!("controlled release gate is never poisoned"));
            while !*released {
                released = gate
                    .1
                    .wait(released)
                    .unwrap_or_else(|_| unreachable!("controlled release gate is never poisoned"));
            }
            active.fetch_sub(1, Ordering::SeqCst);
        })?);
    }
    let result = std::thread::scope(|scope| {
        let service_ready = (0..plan.bulk_limit().get())
            .try_for_each(|_| observed.recv_timeout(Duration::from_secs(5)));
        let (finished, result) = mpsc::channel();
        let cpu = &cpu;
        let worker = scope.spawn(move || {
            let mut inputs = vec![0_u32; 64];
            let mut frame = FrameBatch::new(|value| *value = 42);
            let result = frame
                .start(cpu, &mut inputs)
                .and_then(|()| frame.reclaim(&mut inputs));
            let _sent = finished.send(result);
            inputs
        });
        let frame_ready = result.recv_timeout(Duration::from_secs(5));
        // Always release held calls before reporting a scheduling failure.
        *gate
            .0
            .lock()
            .unwrap_or_else(|_| unreachable!("controlled release gate is never poisoned")) = true;
        gate.1.notify_all();
        let inputs = worker
            .join()
            .unwrap_or_else(|_| unreachable!("frame fixture does not panic"));
        (service_ready, frame_ready, inputs)
    });
    for task in tasks {
        task.join()?;
    }
    result.0?;
    result.1??;
    assert_eq!(result.2, [42; 64]);
    assert_eq!(peak.load(Ordering::SeqCst), plan.bulk_limit().get());
    Ok(())
}
