//! Resumable service ownership, scheduling and failure publication.

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan};
use std::{
    error::Error,
    num::NonZeroUsize,
    ops::ControlFlow,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

/// One lane makes queue-order assertions independent of OS scheduling.
fn cpu(capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(capacity).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

#[test]
fn required_work_runs_between_retirement_steps_without_readmission() -> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(8)?;
    let (release, wait) = mpsc::channel();
    let (started, observed) = mpsc::channel();
    let blocker = cpu.try_submit(move || {
        let _sent = started.send(());
        let _released = wait.recv();
    })?;
    observed.recv_timeout(Duration::from_secs(5))?;
    let (record, order) = mpsc::channel();
    let output = record.clone();
    let mut step = 0;
    let task = cpu
        .try_reserve_for(CpuService::Retirement)?
        .submit_steps(move || {
            let _sent = output.send(step);
            step += 1;
            if step == 4 {
                ControlFlow::Break(step)
            } else {
                ControlFlow::Continue(())
            }
        });
    let mut required = Vec::new();
    for value in [100, 101] {
        let record = record.clone();
        required.push(cpu.try_submit(move || record.send(value))?);
    }
    release.send(())?;
    let actual = (0..6)
        .map(|_| order.recv_timeout(Duration::from_secs(5)))
        .collect::<Result<Vec<_>, _>>()?;
    blocker.join()?;
    for required in required {
        required.join()??;
    }
    assert_eq!(task.join()?, 4);
    cpu.shutdown()?;
    assert_eq!(actual, [0, 100, 1, 101, 2, 3]);
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}

#[test]
fn changing_demand_during_a_running_step_controls_its_next_enqueue() -> Result<(), Box<dyn Error>> {
    for (initial, next, expected) in [
        (CpuService::Speculative, CpuService::Required, [10, 20, 30]),
        (CpuService::Required, CpuService::Speculative, [10, 30, 20]),
    ] {
        let cpu = cpu(8)?;
        let (started, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (record, order) = mpsc::channel();
        let output = record.clone();
        let mut first = true;
        let task = cpu.try_reserve_for(initial)?.submit_steps(move || {
            if first {
                first = false;
                let _sent = started.send(());
                let _released = wait.recv();
                ControlFlow::Continue(())
            } else {
                let _sent = output.send(20);
                ControlFlow::Break(())
            }
        });
        observed.recv_timeout(Duration::from_secs(5))?;
        let optional = cpu.try_submit_for(CpuService::Speculative, {
            let record = record.clone();
            move || record.send(30)
        })?;
        let required = cpu.try_submit(move || record.send(10))?;
        task.set_service(next);
        release.send(())?;
        let actual = (0..3)
            .map(|_| order.recv_timeout(Duration::from_secs(5)))
            .collect::<Result<Vec<_>, _>>()?;
        task.join()?;
        optional.join()??;
        required.join()??;
        assert_eq!(actual, expected);
    }
    Ok(())
}

/// Observes captured state retirement before a failed result is published.
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn a_later_step_panic_retires_captures_and_returns_capacity_before_completion()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu(1)?;
    let retired = Arc::new(AtomicBool::new(false));
    let capture = DropFlag(Arc::clone(&retired));
    let mut calls = 0;
    let task = cpu.try_reserve()?.submit_steps(move || -> ControlFlow<()> {
        std::hint::black_box(&capture);
        calls += 1;
        if calls == 1 {
            ControlFlow::Continue(())
        } else {
            panic!("injected resumable failure")
        }
    });
    assert!(matches!(task.join(), Err(CpuError::TaskPanicked)));
    assert!(retired.load(Ordering::Acquire));
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    Ok(())
}

#[test]
fn shutdown_drains_every_step_even_after_the_result_handle_is_dropped() -> Result<(), Box<dyn Error>>
{
    let mut cpu = cpu(1)?;
    let count = Arc::new(AtomicUsize::new(0));
    let completed = Arc::clone(&count);
    let task = cpu
        .try_reserve_for(CpuService::Retirement)?
        .submit_steps(move || {
            if completed.fetch_add(1, Ordering::AcqRel) == 99 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
    drop(task);
    cpu.shutdown()?;
    assert_eq!(count.load(Ordering::Acquire), 100);
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}
