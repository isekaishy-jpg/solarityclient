//! Owned result readiness, state return and bounded epoch admission.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::time::Duration;

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan, JobOutcome};

/// Explicit small scheduler budget for controlled interleavings.
fn executor() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(3).ok_or(CpuError::InvalidJob)?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
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
    let handle = batch.job(1)?;
    let (ready, observed) = mpsc::sync_channel(1);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            assert!(
                ready
                    .send(batch.with_result(&handle, |job| job.value))
                    .is_ok()
            );
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

#[test]
fn outcome_wait_preserves_payload_and_ignores_unrelated_pending_output()
-> Result<(), Box<dyn Error>> {
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
    let handle = batch.job(1)?;
    let (ready, observed) = mpsc::sync_channel(1);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            assert!(ready.send(batch.wait_for_outcome(&handle)).is_ok());
        });
        let result = observed.recv_timeout(Duration::from_secs(5));
        // Always unblock the worker, including when readiness regresses.
        assert!(release.send(()).is_ok());
        result
    });
    assert_eq!(result??, JobOutcome::Succeeded);
    assert_eq!(batch.try_with_result(&handle, |job| job.value)?, Some(42));
    batch.wait_until_finished()?;
    assert!(batch.is_finished());
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs[0].value, 1);
    assert_eq!(jobs[1].value, 42);
    // Recycling an epoch must not make its previous readiness identity valid.
    jobs[0].wait = None;
    batch.start(&cpu, &mut jobs)?;
    assert!(matches!(
        batch.wait_for_outcome(&handle),
        Err(CpuError::StaleJob)
    ));
    batch.reclaim(&mut jobs)?;
    Ok(())
}

#[test]
fn terminal_wait_rejects_an_open_producer_without_closing_it() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let mut batch = FrameBatch::new(run);
    batch.begin(&cpu, FrameBatchPlan::new(1, 0))?;
    assert!(matches!(
        batch.wait_until_finished(),
        Err(CpuError::BatchOpen)
    ));
    let handle = batch.push(&mut Some(Job {
        wait: None,
        value: 7,
    }))?;
    assert_eq!(batch.wait_for_outcome(&handle)?, JobOutcome::Succeeded);
    assert!(matches!(
        batch.wait_until_finished(),
        Err(CpuError::BatchOpen)
    ));
    batch.close();
    batch.wait_until_finished()?;
    let mut jobs = Vec::new();
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs[0].value, 8);
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
    assert_eq!(
        batch.wait_for_outcome(&batch.job(0)?)?,
        JobOutcome::Panicked
    );
    batch.wait_until_finished()?;
    assert!(jobs.is_empty());
    assert!(matches!(
        batch.reclaim(&mut jobs),
        Err(CpuError::TaskPanicked)
    ));
    assert_eq!(jobs, [vec![1, 7]]);
    Ok(())
}

/// Domain failures carry modified owned payloads through terminal readiness.
fn fail_domain(job: &mut Vec<usize>) -> JobOutcome {
    job.push(9);
    JobOutcome::Failed
}

#[test]
fn failed_outcome_wait_leaves_domain_state_for_reclamation() -> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let mut batch = FrameBatch::with_outcome(fail_domain);
    let mut jobs = vec![vec![2]];
    batch.start(&cpu, &mut jobs)?;
    assert_eq!(batch.wait_for_outcome(&batch.job(0)?)?, JobOutcome::Failed);
    batch.wait_until_finished()?;
    assert!(jobs.is_empty());
    assert!(matches!(batch.reclaim(&mut jobs), Err(CpuError::JobFailed)));
    assert_eq!(jobs, [vec![2, 9]]);
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
        assert_eq!(
            second.with_result(&second.job(0)?, |job| job.value)?,
            expected
        );
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
    let handle = batch.job(0)?;
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        batch.with_result(&handle, |job| {
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
    batch.begin(&cpu, solarity_cpu::FrameBatchPlan::new(64, 0))?;
    for index in 0..64 {
        let mut job = Some(Job {
            wait: None,
            value: index,
        });
        let handle = batch.push(&mut job)?;
        assert!(job.is_none());
        assert_eq!(batch.with_result(&handle, |job| job.value)?, index + 1);
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
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or(CpuError::InvalidJob)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (send, receive) = mpsc::sync_channel::<solarity_cpu::CpuTask<usize>>(1);
    let first = cpu.try_submit(move || receive.recv().map(|task| task.join()))?;
    let second = cpu.try_submit(|| 42)?;
    send.send(second)?;
    assert!(matches!(first.join()??, Err(CpuError::WorkerWait)));
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn worker_readiness_waits_reject_queued_frame_work_and_return_ownership()
-> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or(CpuError::InvalidJob)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (send, receive) = mpsc::sync_channel::<FrameBatch<Job>>(1);
    let (entered, running) = mpsc::sync_channel(1);
    let task = cpu.try_submit(move || {
        assert!(entered.send(()).is_ok());
        receive.recv().map(|batch| {
            let outcome = batch
                .job(0)
                .and_then(|handle| batch.wait_for_outcome(&handle));
            let terminal = batch.wait_until_finished();
            (batch, outcome, terminal)
        })
    })?;
    running.recv_timeout(Duration::from_secs(5))?;
    let mut batch = FrameBatch::new(run);
    let mut jobs = vec![Job {
        wait: None,
        value: 9,
    }];
    batch.start(&cpu, &mut jobs)?;
    assert!(send.send(batch).is_ok());
    let (mut batch, outcome, terminal) = task.join()??;
    assert!(matches!(outcome, Err(CpuError::WorkerWait)));
    assert!(matches!(terminal, Err(CpuError::WorkerWait)));
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs[0].value, 10);
    Ok(())
}

/// A full destination refuses before moving any worker-owned value and remains retryable.
#[test]
fn admitted_reclamation_preserves_inputs_on_destination_pressure() -> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuBuffer, CpuStorageClass, CpuStorageKind};
    let cpu = executor()?;
    let mut batch = FrameBatch::new(|value: &mut usize| *value += 10);
    let mut input = vec![1, 2, 3];
    batch.start(&cpu, &mut input)?;
    let mut output = CpuBuffer::default();
    output.reserve(
        cpu.storage(),
        CpuStorageClass::Frame,
        CpuStorageKind::Result,
        3,
    )?;
    output.push(99)?;
    assert!(matches!(
        batch.reclaim_into(&mut output.writer()),
        Err(CpuError::OutputCapacity { .. })
    ));
    assert_eq!(&*output, &[99]);
    output.clear();
    let pointer = output.as_ptr();
    let before = cpu.storage().snapshot().used(CpuStorageClass::Frame);
    batch.reclaim_into(&mut output.writer())?;
    assert_eq!(&*output, &[11, 12, 13]);
    assert_eq!(output.as_ptr(), pointer);
    assert_eq!(
        cpu.storage().snapshot().used(CpuStorageClass::Frame),
        before
    );
    assert_eq!(output.pop(), Some(13));
    output.truncate(1);
    assert_eq!(&*output, &[11]);
    Ok(())
}

#[test]
fn admitted_inputs_preserve_storage_through_refusal_execution_and_return()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{
        CpuBuffer, CpuStorageClass as Class, CpuStorageKind as Kind, FrameGraphTemplate,
    };
    let mut cpu = executor()?;
    let mut jobs = CpuBuffer::<usize>::default();
    jobs.reserve(cpu.storage(), Class::Frame, Kind::Result, 3)?;
    jobs.extend_from_slice(&[3, 5, 7])?;
    let address = jobs.as_ptr();
    let mut batch = FrameBatch::new(|value| *value += 10);
    let storage = cpu.storage().snapshot().used(Class::Frame);
    assert!(matches!(
        batch.start_graph(&cpu, &FrameGraphTemplate::independent(2), &mut jobs, &[]),
        Err(CpuError::GraphInputCount)
    ));
    assert_eq!(&*jobs, &[3, 5, 7]);
    assert_eq!(jobs.as_ptr(), address);
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), storage);
    for step in 1..=8 {
        batch.start(&cpu, &mut jobs)?;
        assert!(jobs.is_empty());
        assert_eq!(jobs.as_ptr(), address);
        batch.reclaim_into(&mut jobs.writer())?;
        assert_eq!(&*jobs, &[3 + 10 * step, 5 + 10 * step, 7 + 10 * step]);
        assert_eq!(jobs.as_ptr(), address);
    }
    drop((jobs, batch));
    cpu.shutdown()?;
    let budget = cpu.storage().clone();
    drop(cpu);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn admitted_slot_initialization_refuses_before_calling_the_factory() -> Result<(), CpuError> {
    use solarity_cpu::{
        CpuBuffer, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
        CpuStoragePlan,
    };
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(3 * size_of::<usize>(), 0, 0));
    let mut values = CpuBuffer::default();
    values.reserve(&budget, Class::Frame, Kind::Metadata, 3)?;
    values.push(7_usize)?;
    let address = values.as_ptr();
    let calls = std::cell::Cell::new(0);
    let make = || {
        calls.set(calls.get() + 1);
        11
    };
    assert!(values.resize_with(4, make).is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(&*values, &[7]);
    values.resize_with(3, make)?;
    assert_eq!(calls.get(), 2);
    assert_eq!(&*values, &[7, 11, 11]);
    values.resize_with(1, make)?;
    assert_eq!(calls.get(), 2);
    assert_eq!(values.as_ptr(), address);
    assert_eq!(budget.snapshot().used(Class::Frame), 3 * size_of::<usize>());
    drop(values);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}
