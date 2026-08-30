//! External lifecycle tests for the application-owned CPU pool.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig};

/// Creates explicit nonzero test capacity without a repository default.
fn config(worker_count: usize, max_in_flight: usize) -> CpuPoolConfig {
    let worker_count = match NonZeroUsize::new(worker_count) {
        Some(value) => value,
        None => unreachable!("test worker count is a nonzero literal"),
    };
    let max_in_flight = match NonZeroUsize::new(max_in_flight) {
        Some(value) => value,
        None => unreachable!("test task bound is a nonzero literal"),
    };
    CpuPoolConfig::new(worker_count, max_in_flight)
}

/// Submitted work produces one typed result and releases its admission slot.
#[test]
fn submitted_task_returns_its_owned_result() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(2, 4))?;
    let task = executor.try_submit(|| 21 * 2)?;

    assert_eq!(executor.worker_count(), 2);
    assert_eq!(executor.snapshot()?.max_in_flight().get(), 4);
    assert_eq!(task.join()?, 42);
    assert_eq!(executor.snapshot()?.in_flight(), 0);
    executor.shutdown()?;
    Ok(())
}

/// Capacity rejection is immediate and does not enqueue hidden work.
#[test]
fn in_flight_bound_applies_to_running_and_queued_work() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(1, 1))?;
    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let task = executor.try_submit(move || {
        let _started = started_sender.send(());
        let _released = release_receiver.recv();
    })?;
    started_receiver.recv()?;

    assert!(matches!(
        executor.try_submit(|| 7),
        Err(CpuError::AtCapacity { limit }) if limit.get() == 1
    ));

    release_sender.send(())?;
    task.join()?;
    executor.shutdown()?;
    Ok(())
}

/// Shutdown drains admitted work and permanently closes admission.
#[test]
fn shutdown_waits_for_admitted_work_and_rejects_new_tasks() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(1, 2))?;
    let completed = Arc::new(AtomicBool::new(false));
    let completed_by_task = Arc::clone(&completed);
    let task = executor.try_submit(move || {
        completed_by_task.store(true, Ordering::Release);
    })?;

    executor.shutdown()?;

    assert!(completed.load(Ordering::Acquire));
    assert_eq!(task.join()?, ());
    assert!(matches!(
        executor.try_submit(|| ()),
        Err(CpuError::ShuttingDown)
    ));
    assert!(!executor.snapshot()?.is_accepting());
    Ok(())
}

/// Dropping a result handle never detaches work from executor shutdown.
#[test]
fn discarded_result_remains_owned_until_shutdown() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(1, 2))?;
    let completed = Arc::new(AtomicBool::new(false));
    let completed_by_task = Arc::clone(&completed);
    let task = executor.try_submit(move || {
        completed_by_task.store(true, Ordering::Release);
    })?;
    drop(task);

    executor.shutdown()?;

    assert!(completed.load(Ordering::Acquire));
    Ok(())
}

/// Task unwinding is reported without destroying the private worker pool.
#[test]
fn task_panic_is_typed_and_pool_remains_usable() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(1, 2))?;
    let failed = executor.try_submit(|| panic!("synthetic task failure"))?;

    assert!(matches!(failed.join(), Err(CpuError::TaskPanicked)));
    let succeeding = executor.try_submit(|| 1234)?;
    assert_eq!(succeeding.join()?, 1234);
    executor.shutdown()?;
    Ok(())
}

/// Completion state can be polled without consuming the eventual result.
#[test]
fn completion_can_be_observed_without_consuming_result() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(1, 1))?;
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let task = executor.try_submit(move || {
        let _released = release_receiver.recv();
        99
    })?;

    assert!(!task.is_finished());
    release_sender.send(())?;
    assert_eq!(task.join()?, 99);
    executor.shutdown()?;
    Ok(())
}
