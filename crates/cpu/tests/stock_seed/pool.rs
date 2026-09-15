//! External lifecycle tests for the application-owned CPU pool.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};

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
    CpuPoolConfig::new(
        worker_count,
        max_in_flight,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    )
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

#[test]
fn reserved_admission_keeps_inputs_with_the_producer_until_submission() -> Result<(), Box<dyn Error>>
{
    let mut executor = CpuExecutor::new(config(1, 1))?;
    let permit = executor.try_reserve()?;
    assert_eq!(executor.snapshot()?.in_flight(), 1);
    let input = String::from("retained input");
    assert!(matches!(
        executor.try_reserve(),
        Err(CpuError::AtCapacity { .. })
    ));
    assert_eq!(input, "retained input");
    drop(permit);
    assert_eq!(executor.snapshot()?.in_flight(), 0);
    let task = executor.try_reserve()?.submit(move || input);
    assert_eq!(task.join()?, "retained input");
    assert_eq!(executor.snapshot()?.in_flight(), 0);
    executor.shutdown()?;
    assert!(matches!(
        executor.try_reserve(),
        Err(CpuError::ShuttingDown)
    ));
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

/// Speculative admission never fills a queue behind the flexible worker.
#[test]
fn speculative_admission_stops_while_flexible_service_is_occupied() -> Result<(), Box<dyn Error>> {
    let mut executor = CpuExecutor::new(config(8, 32))?;
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let task = executor.try_submit(move || release_receiver.recv())?;
    assert!(!executor.can_admit_speculative()?);
    release_sender.send(())?;
    task.join()??;
    assert!(executor.can_admit_speculative()?);
    executor.shutdown()?;
    Ok(())
}

/// Occupied archive workers and exhausted job capacity cannot hold a frame join.
#[test]
fn frame_batch_completes_before_blocked_background_jobs_are_released() -> Result<(), Box<dyn Error>>
{
    let mut executor = CpuExecutor::new(config(4, 1))?;
    assert_eq!(executor.background_worker_count(), 1);
    assert_eq!(executor.frame_worker_count(), 3);
    let started = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let mut tasks = Vec::new();
    for _ in 0..1 {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        tasks.push(executor.try_submit(move || {
            started.wait();
            release.wait();
        })?);
    }
    started.wait();
    assert!(matches!(
        executor.try_reserve(),
        Err(CpuError::AtCapacity { .. })
    ));
    let mut values = vec![0; 128];
    let (sender, receiver) = mpsc::sync_channel(1);
    let completed_before_release = std::thread::scope(|scope| {
        let frame = scope.spawn(|| {
            let mut batch = solarity_cpu::FrameBatch::new(|value| *value = 42);
            let result = batch
                .start(&executor, &mut values)
                .and_then(|()| batch.reclaim(&mut values));
            let _received = sender.send(result);
        });
        // The timeout bounds a broken scheduler; it is not a performance threshold.
        let completed = receiver.recv_timeout(std::time::Duration::from_secs(5));
        release.wait();
        assert!(frame.join().is_ok());
        completed
    });
    for task in tasks {
        task.join()?;
    }
    completed_before_release??;
    assert_eq!(values, [42; 128]);
    executor.shutdown()?;
    assert!(matches!(
        solarity_cpu::FrameBatch::new(|_: &mut i32| {}).start(&executor, &mut values),
        Err(CpuError::ShuttingDown)
    ));
    Ok(())
}

/// A one-worker budget never adds a hidden second pool or waits for archive jobs.
#[test]
fn single_worker_frame_batch_uses_its_only_worker() -> Result<(), Box<dyn Error>> {
    let executor = CpuExecutor::new(config(1, 1))?;
    let caller = std::thread::current().id();
    let mut owners = vec![None; 8];
    let mut batch =
        solarity_cpu::FrameBatch::new(|owner| *owner = Some(std::thread::current().id()));
    batch.start(&executor, &mut owners)?;
    batch.reclaim(&mut owners)?;
    assert_eq!(executor.background_worker_count(), 1);
    assert_eq!(executor.frame_worker_count(), 0);
    assert!(
        owners
            .iter()
            .all(|owner| owner.is_some_and(|id| id != caller))
    );
    Ok(())
}
