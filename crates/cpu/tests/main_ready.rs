//! Main-only consumers keep non-Send state while workers deliver bounded notices.

use solarity_cpu::{
    CompletionPort, CoordinatorNotifier, CpuError, CpuExecutor, CpuPoolConfig, CpuStorageClass,
    CpuStoragePlan, FrameBatch, FrameBatchPlan, JobOutcome,
};
use std::{
    cell::Cell,
    error::Error,
    num::NonZeroUsize,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Notification counts are observed after publication, not used as readiness state.
struct Notices(AtomicUsize);
impl CoordinatorNotifier for Notices {
    fn notify(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn cpu() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or(CpuError::InvalidJob)?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))
}

#[test]
fn worker_main_worker_chain_preserves_non_send_ownership() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut first = FrameBatch::new(|value: &mut usize| *value += 1);
    first.begin(&cpu, FrameBatchPlan::new(1, 0))?;
    let first_job = first.push(&mut Some(8))?;
    first.close();
    let mut ready = cpu.main_ready_queue();
    ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(42, &[first.completion()?])?;
    let published = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut publication = published.producer()?;
    let mut next = FrameBatch::new(|value: &mut usize| *value *= 2);
    next.begin_when(&cpu, FrameBatchPlan::new(1, 0), &published.readiness())?;
    let next_job = next.push(&mut Some(9))?;
    next.close();
    assert!(next.outcome(&next_job)?.is_none());
    ready.wait_until_ready()?;
    let notice = ready.take_ready().ok_or("main continuation")?;
    assert_eq!(notice.key(), 42);
    assert_eq!(notice.outcome(), JobOutcome::Succeeded);
    let main_state = Rc::new(Cell::new(first.with_result(&first_job, |v| *v)?));
    main_state.set(main_state.get() + 1);
    publication.complete(JobOutcome::Succeeded)?;
    assert_eq!(next.with_result(&next_job, |v| *v)?, 18);
    assert_eq!(main_state.get(), 10);
    assert!(!ready.has_ready());
    first.reclaim(&mut Vec::new())?;
    next.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn failed_fan_in_releases_a_never_ready_sibling_and_wakes_main() -> Result<(), Box<dyn Error>> {
    let notifier = Arc::new(Notices(AtomicUsize::new(0)));
    let cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::MIN,
            CpuStoragePlan::new(64 << 20, 64 << 20, 0),
        ),
        notifier.clone(),
    )?;
    let failed = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let pending = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = failed.producer()?;
    let _pending_owner = pending.producer()?;
    let mut ready = cpu.main_ready_queue();
    ready.begin(1, 2, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(9, &[failed.readiness(), pending.readiness()])?;
    producer.complete(JobOutcome::Failed)?;
    assert!(ready.has_ready());
    assert!(notifier.0.load(Ordering::SeqCst) > 0);
    assert_eq!(
        ready.take_ready().ok_or("failure notice")?.outcome(),
        JobOutcome::DependencyFailed
    );
    let mut other = cpu.main_ready_queue();
    other.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    other.watch(11, &[pending.readiness()])?;
    assert!(
        !other.has_ready(),
        "failure released the sibling's subscriber slot"
    );
    Ok(())
}

#[test]
fn cancellation_reuse_and_owner_drop_leave_shared_producer_alive() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = port.producer()?;
    let mut ready = cpu.main_ready_queue();
    ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(1, &[port.readiness()])?;
    ready.cancel();
    assert_eq!(port.readiness().outcome()?, None);
    ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(2, &[port.readiness()])?;
    producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(ready.take_ready().ok_or("new epoch")?.key(), 2);
    assert!(ready.take_ready().is_none());
    drop(ready);
    assert_eq!(port.readiness().outcome()?, Some(JobOutcome::Succeeded));
    Ok(())
}

#[test]
fn refused_fan_in_rolls_back_all_subscriptions_without_consuming_capacity()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let available = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let full = CompletionPort::new(0, cpu.storage(), CpuStorageClass::Frame)?;
    let mut ready = cpu.main_ready_queue();
    ready.begin(1, 2, cpu.storage(), CpuStorageClass::Frame)?;
    assert!(matches!(
        ready.watch(1, &[available.readiness(), full.readiness()]),
        Err(CpuError::ReadinessCapacity)
    ));
    ready.watch(2, &[available.readiness()])?;
    assert!(matches!(ready.watch(3, &[]), Err(CpuError::BatchCapacity)));
    drop(available.producer()?);
    assert_eq!(ready.take_ready().ok_or("abandonment")?.key(), 2);
    assert!(ready.take_ready().is_none());
    Ok(())
}

#[test]
fn publication_racing_registration_delivers_once_without_polling_sleep()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut ready = cpu.main_ready_queue();
    let mut port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    for iteration in 0..128 {
        if iteration != 0 {
            port.restart(1, cpu.storage(), CpuStorageClass::Frame)?;
        }
        ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
        let mut producer = port.producer()?;
        let token = port.readiness();
        std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
            let publication = scope.spawn(move || producer.complete(JobOutcome::Succeeded));
            ready.watch(iteration, &[token])?;
            ready.wait_until_ready()?;
            assert_eq!(ready.take_ready().ok_or("racing notice")?.key(), iteration);
            assert!(ready.take_ready().is_none());
            publication.join().map_err(|_| "producer panicked")??;
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn ready_work_precedes_wait_and_incomplete_epoch_cannot_be_overwritten()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut ready = cpu.main_ready_queue();
    ready.begin(2, 1, cpu.storage(), CpuStorageClass::Frame)?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let producer = port.producer()?;
    ready.watch(1, &[port.readiness()])?;
    ready.watch(2, &[])?;
    ready.wait_until_ready()?;
    assert_eq!(ready.take_ready().ok_or("independent main work")?.key(), 2);
    assert!(matches!(
        ready.begin(1, 0, cpu.storage(), CpuStorageClass::Frame),
        Err(CpuError::BatchActive)
    ));
    drop(producer);
    assert_eq!(
        ready.take_ready().ok_or("cancelled parent")?.outcome(),
        JobOutcome::DependencyFailed
    );
    assert!(matches!(ready.wait_until_ready(), Err(CpuError::BatchOpen)));
    ready.cancel();
    assert!(matches!(
        ready.wait_until_ready(),
        Err(CpuError::BatchInactive)
    ));
    Ok(())
}

#[test]
fn full_frame_capacity_still_admits_its_main_consumer() -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let source = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = source.producer()?;
    let mut batch = FrameBatch::new(|v: &mut usize| *v += 1);
    batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &source.readiness())?;
    let job = batch.push(&mut Some(41))?;
    batch.close();
    let mut other = FrameBatch::new(|v: &mut usize| *v += 1);
    assert!(matches!(
        other.begin(&cpu, FrameBatchPlan::new(1, 0)),
        Err(CpuError::AtCapacity { .. })
    ));
    let mut ready = cpu.main_ready_queue();
    ready.begin(1, 1, cpu.storage(), CpuStorageClass::Frame)?;
    ready.watch(7, &[batch.completion()?])?;
    producer.complete(JobOutcome::Succeeded)?;
    ready.wait_until_ready()?;
    assert_eq!(
        ready
            .take_ready()
            .ok_or("consumer reserved independently")?
            .key(),
        7
    );
    assert_eq!(batch.with_result(&job, |value| *value)?, 42);
    batch.reclaim(&mut Vec::new())?;
    other.begin(&cpu, FrameBatchPlan::new(1, 0))?;
    other.close();
    other.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn workers_cannot_park_for_main_continuation_readiness() -> Result<(), Box<dyn Error>> {
    let cpu = Arc::new(cpu()?);
    let worker_cpu = Arc::clone(&cpu);
    let result = cpu
        .try_submit(move || -> Result<(), CpuError> {
            let source = CompletionPort::new(1, worker_cpu.storage(), CpuStorageClass::Frame)?;
            let _producer = source.producer()?;
            let mut ready = worker_cpu.main_ready_queue();
            ready.begin(1, 1, worker_cpu.storage(), CpuStorageClass::Frame)?;
            ready.watch(0, &[source.readiness()])?;
            ready.wait_until_ready()
        })?
        .join()?;
    assert!(matches!(result, Err(CpuError::WorkerWait)));
    Ok(())
}
