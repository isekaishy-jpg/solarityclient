//! Discovered source gates release workers while retaining typed owned continuations.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService,
    CpuStorageClass, CpuStoragePlan, CpuTaskDependency, CpuTaskStep, JobOutcome,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

/// One worker makes a marker prove that a previous service has returned its lane.
fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

/// Successful, failed and abandoned sources all resume the typed source consumer.
#[test]
fn a_pending_dependency_releases_the_only_worker_and_preserves_service_identity()
-> Result<(), Box<dyn Error>> {
    for outcome in [
        JobOutcome::Succeeded,
        JobOutcome::Failed,
        JobOutcome::Cancelled,
    ] {
        let cpu = cpu()?;
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let mut producer = port.producer()?;
        let token = port.readiness();
        let mut dependency = Some(CpuTaskDependency::new(&token)?);
        let (entered, observed) = mpsc::channel();
        let mut identity = None;
        let task = cpu
            .try_reserve()?
            .submit_resumable_with_context(move |context| {
                if let Some(dependency) = dependency.take() {
                    identity = Some(context.identity());
                    let _sent = entered.send(());
                    CpuTaskStep::Wait(dependency)
                } else {
                    assert_eq!(identity, Some(context.identity()));
                    CpuTaskStep::Complete(token.outcome())
                }
            });
        observed.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
        assert!(!task.is_finished());
        assert_eq!(cpu.snapshot()?.in_flight(), 1);
        producer.complete(outcome)?;
        assert_eq!(task.join()??, Some(outcome));
        assert_eq!(cpu.snapshot()?.in_flight(), 0);
    }
    Ok(())
}

/// Reservation before publication and binding after publication cannot lose readiness.
#[test]
fn publication_before_wait_registration_still_resumes_once() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
    let mut producer = port.producer()?;
    let mut dependency = Some(CpuTaskDependency::new(&port.readiness())?);
    producer.complete(JobOutcome::Succeeded)?;
    let mut calls = 0;
    let task = cpu.try_reserve()?.submit_resumable_with_context(move |_| {
        calls += 1;
        match dependency.take() {
            Some(dependency) => CpuTaskStep::Wait(dependency),
            None => CpuTaskStep::Complete(calls),
        }
    });
    assert_eq!(task.join()?, 2);
    Ok(())
}

/// Both sides of the cancellation/park race retain worker-side cleanup.
#[test]
fn cancellation_before_and_after_suspension_returns_owned_inputs() -> Result<(), Box<dyn Error>> {
    for cancel_before_park in [false, true] {
        let cpu = cpu()?;
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let _producer = port.producer()?;
        let mut dependency = Some(CpuTaskDependency::new(&port.readiness())?);
        let (entered, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let mut input = Some(vec![2, 3, 5]);
        let task = cpu
            .try_reserve()?
            .submit_resumable_with_context(move |context| {
                if let Some(dependency) = dependency.take() {
                    let _sent = entered.send(());
                    if cancel_before_park {
                        let _released = wait.recv_timeout(Duration::from_secs(5));
                    }
                    CpuTaskStep::Wait(dependency)
                } else {
                    assert!(context.is_cancelled());
                    CpuTaskStep::Complete(input.take())
                }
            });
        observed.recv_timeout(Duration::from_secs(5))?;
        if !cancel_before_park {
            cpu.try_submit(|| ())?.join()?;
        }
        task.cancel();
        if cancel_before_park {
            release.send(())?;
        }
        assert_eq!(task.join()?, Some(vec![2, 3, 5]));
        assert_eq!(port.readiness().outcome()?, None);
        // Cancellation returned the port's bounded subscriber reservation.
        let _next = CpuTaskDependency::new(&port.readiness())?;
    }
    Ok(())
}

/// An indefinitely pending external source cannot strand executor shutdown.
#[test]
fn shutdown_and_dropped_consumers_resume_cleanup_without_source_completion()
-> Result<(), Box<dyn Error>> {
    for drop_consumer in [false, true] {
        let mut cpu = cpu()?;
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let _producer = port.producer()?;
        let mut dependency = Some(CpuTaskDependency::new(&port.readiness())?);
        let (entered, observed) = mpsc::channel();
        let (completed, result) = mpsc::channel();
        let task = cpu
            .try_reserve()?
            .submit_resumable_with_context(move |context| {
                if let Some(dependency) = dependency.take() {
                    let _sent = entered.send(());
                    CpuTaskStep::Wait(dependency)
                } else {
                    let _sent = completed.send(context.is_cancelled());
                    CpuTaskStep::Complete(())
                }
            });
        observed.recv_timeout(Duration::from_secs(5))?;
        cpu.try_submit(|| ())?.join()?;
        let task = if drop_consumer {
            drop(task);
            None
        } else {
            Some(task)
        };
        cpu.shutdown()?;
        assert!(result.recv_timeout(Duration::from_secs(5))?);
        if let Some(task) = task {
            task.join()?;
        }
        assert_eq!(cpu.snapshot()?.in_flight(), 0);
    }
    Ok(())
}

/// Successive resource discoveries reuse one slot, including terminal failure.
#[test]
fn repeated_dependencies_preserve_order_and_worker_panic_returns_admission()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let ports = (0..3)
        .map(|_| CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required))
        .collect::<Result<Vec<_>, _>>()?;
    let mut producers = ports
        .iter()
        .map(CompletionPort::producer)
        .collect::<Result<Vec<_>, _>>()?;
    let mut dependencies = ports
        .iter()
        .map(|port| CpuTaskDependency::new(&port.readiness()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter();
    let (record, turns) = mpsc::channel();
    let mut index = 0;
    let task = cpu
        .try_reserve_for(CpuService::Speculative)?
        .submit_resumable_with_context(move |_| -> CpuTaskStep<()> {
            let _sent = record.send(index);
            index += 1;
            match dependencies.next() {
                Some(dependency) => CpuTaskStep::Wait(dependency),
                None => panic!("injected resumed failure"),
            }
        });
    for (index, producer) in producers.iter_mut().enumerate() {
        assert_eq!(turns.recv_timeout(Duration::from_secs(5))?, index);
        cpu.try_submit(|| ())?.join()?;
        assert!(!task.is_finished());
        task.set_service(CpuService::Required);
        producer.complete(JobOutcome::Succeeded)?;
    }
    assert!(matches!(task.join(), Err(CpuError::TaskPanicked)));
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    assert_eq!(cpu.try_submit(|| 17)?.join()?, 17);
    Ok(())
}

/// A live dependency carries promotion and withdrawal while its consumer is asleep.
#[test]
fn suspended_consumer_demand_reaches_the_shared_producer_without_a_worker_turn()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Required)?;
    let _producer = port.producer()?;
    let producer_permit = cpu.try_reserve_for(CpuService::Speculative)?;
    let producer_control = producer_permit.service_control();
    let demand = solarity_cpu::CpuServiceDemand::default();
    let interest = demand.subscribe(CpuService::Speculative);
    assert!(demand.bind(producer_control.clone()));
    let mut dependency = Some(CpuTaskDependency::new(&port.readiness())?.with_demand(interest));
    let (entered, observed) = mpsc::channel();
    let task = cpu
        .try_reserve_for(CpuService::Required)?
        .submit_resumable_with_context(move |context| match dependency.take() {
            Some(dependency) => {
                let _sent = entered.send(());
                CpuTaskStep::Wait(dependency)
            }
            None => CpuTaskStep::Complete(context.is_cancelled()),
        });
    observed.recv_timeout(Duration::from_secs(5))?;
    cpu.try_submit(|| ())?.join()?;
    assert_eq!(producer_control.service(), CpuService::Required);
    task.set_service(CpuService::Speculative);
    assert_eq!(producer_control.service(), CpuService::Speculative);
    task.set_service(CpuService::Required);
    assert_eq!(producer_control.service(), CpuService::Required);
    task.cancel();
    assert!(task.join()?);
    drop(producer_permit);
    Ok(())
}
