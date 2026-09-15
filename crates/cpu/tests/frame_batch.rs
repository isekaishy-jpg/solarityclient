//! Owned result readiness, state return and bounded epoch admission.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::time::Duration;

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig, FrameBatch};

/// Explicit small scheduler budget for controlled interleavings.
fn executor() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::new(3).ok_or(CpuError::InvalidJob)?,
        NonZeroUsize::MIN,
    ))
}

/// A blocked producer and an unrelated completed output share one admitted epoch.
struct Job {
    wait: Option<mpsc::Receiver<()>>,
    value: usize,
}

/// Test kernel whose only blocking input is released by the test owner.
fn run(job: &mut Job) {
    if let Some(wait) = &job.wait {
        assert!(wait.recv().is_ok());
    }
    job.value += 1;
}

#[test]
fn independent_output_is_consumable_before_another_job_finishes() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let (release, wait) = mpsc::sync_channel(1);
    let mut jobs = vec![
        Job {
            wait: Some(wait),
            value: 0,
        },
        Job {
            wait: None,
            value: 41,
        },
    ];
    let mut batch = FrameBatch::new(run);
    batch.start(&cpu, &mut jobs)?;
    assert!(jobs.is_empty());
    let (ready, observed) = mpsc::sync_channel(1);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            assert!(ready.send(batch.with_result(1, |job| job.value)).is_ok());
        });
        let result = observed.recv_timeout(Duration::from_secs(5));
        assert!(release.send(()).is_ok());
        result
    });
    assert_eq!(result??, 42);
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs[0].value, 1);
    assert_eq!(jobs[1].value, 42);
    Ok(())
}

/// Mutates state before an intentional failure to verify unconditional recovery.
fn fail(job: &mut Vec<usize>) {
    job.push(7);
    panic!("fixture worker failure");
}

#[test]
fn panic_returns_owned_storage_before_reporting_failure() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let mut batch = FrameBatch::new(fail);
    let mut jobs = vec![vec![1]];
    batch.start(&cpu, &mut jobs)?;
    assert!(matches!(
        batch.reclaim(&mut jobs),
        Err(CpuError::TaskPanicked)
    ));
    assert_eq!(jobs, [vec![1, 7]]);
    Ok(())
}

#[test]
fn admission_failure_preserves_inputs_and_reuse_preserves_outputs() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let (release, wait) = mpsc::sync_channel(1);
    let mut first = FrameBatch::new(run);
    let mut first_jobs = vec![Job {
        wait: Some(wait),
        value: 0,
    }];
    first.start(&cpu, &mut first_jobs)?;
    let mut second = FrameBatch::new(run);
    let mut jobs = vec![Job {
        wait: None,
        value: 0,
    }];
    assert!(matches!(
        second.start(&cpu, &mut jobs),
        Err(CpuError::AtCapacity { .. })
    ));
    assert_eq!(jobs.len(), 1);
    release.send(())?;
    first.reclaim(&mut first_jobs)?;
    for expected in 1..=100 {
        second.start(&cpu, &mut jobs)?;
        assert_eq!(second.with_result(0, |job| job.value)?, expected);
        second.reclaim(&mut jobs)?;
        assert_eq!(jobs[0].value, expected);
    }
    Ok(())
}

#[test]
fn consumer_unwind_returns_state_and_shutdown_drains_live_batch() -> Result<(), Box<dyn Error>> {
    let mut cpu = executor()?;
    let mut batch = FrameBatch::new(run);
    let mut jobs = vec![Job {
        wait: None,
        value: 12,
    }];
    batch.start(&cpu, &mut jobs)?;
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        batch.with_result(0, |job| {
            job.value = 99;
            panic!("fixture consumer failure");
        })
    }));
    assert!(failed.is_err());
    cpu.shutdown()?;
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs[0].value, 99);
    assert!(matches!(
        batch.start(&cpu, &mut jobs),
        Err(CpuError::ShuttingDown)
    ));
    Ok(())
}

#[test]
fn incremental_producer_can_resume_after_workers_have_drained() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let mut batch = FrameBatch::new(run);
    let mut jobs = Vec::new();
    batch.begin(&cpu)?;
    for index in 0..64 {
        let mut job = Some(Job {
            wait: None,
            value: index,
        });
        assert_eq!(batch.push(&mut job)?, index);
        assert!(job.is_none());
        assert_eq!(batch.with_result(index, |job| job.value)?, index + 1);
    }
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs.len(), 64);
    assert!(
        jobs.iter()
            .enumerate()
            .all(|(index, job)| job.value == index + 1)
    );
    let mut rejected = Some(Job {
        wait: None,
        value: 9,
    });
    assert!(matches!(
        batch.push(&mut rejected),
        Err(CpuError::BatchInactive)
    ));
    assert!(rejected.is_some());
    Ok(())
}

#[test]
fn worker_cannot_deadlock_its_lane_by_joining_a_queued_task() -> Result<(), Box<dyn Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(2).ok_or(CpuError::InvalidJob)?,
    ))?;
    let (send, receive) = mpsc::sync_channel::<solarity_cpu::CpuTask<usize>>(1);
    let first = cpu.try_submit(move || receive.recv().map(|task| task.join()))?;
    let second = cpu.try_submit(|| 42)?;
    send.send(second)?;
    assert!(matches!(first.join()??, Err(CpuError::WorkerWait)));
    cpu.shutdown()?;
    Ok(())
}
