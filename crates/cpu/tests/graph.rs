//! Reusable graph binding, heterogeneous fan-in and failure preserve owned inputs.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan,
    FrameGraphTemplate, JobOutcome,
};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// One worker exposes dependency waits that would otherwise be masked by spare lanes.
fn cpu() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(16).ok_or(CpuError::InvalidJob)?,
    ))
}

#[test]
fn every_external_input_must_succeed_before_a_phase_uses_the_worker() -> Result<(), Box<dyn Error>>
{
    let cpu = cpu()?;
    let first = CompletionPort::new(1)?;
    let second = CompletionPort::new(1)?;
    let mut first_producer = first.producer()?;
    let mut second_producer = second.producer()?;
    let mut batch = FrameBatch::new(|value| *value += 1);
    batch.begin_after(
        &cpu,
        FrameBatchPlan::new(1, 0),
        &[first.readiness(), second.readiness()],
    )?;
    let job = batch.push(&mut Some(9))?;
    batch.close();
    second_producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(batch.outcome(&job)?, None);
    assert_eq!(cpu.try_submit(|| 7)?.join()?, 7);
    first_producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(batch.with_result(&job, |value| *value)?, 10);
    batch.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn failed_fan_in_releases_other_subscriptions_and_preserves_unstarted_inputs()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let failed = CompletionPort::new(1)?;
    let pending = CompletionPort::new(1)?;
    failed.producer()?.complete(JobOutcome::Failed)?;
    let mut batch = FrameBatch::new(|value| *value += 1);
    // Failure is delivered while binding the first edge, before the next binder.
    batch.begin_after(
        &cpu,
        FrameBatchPlan::new(1, 0),
        &[failed.readiness(), pending.readiness()],
    )?;
    batch.push(&mut Some(8))?;
    let mut inputs = Vec::new();
    assert!(matches!(
        batch.reclaim(&mut inputs),
        Err(CpuError::DependencyFailed)
    ));
    assert_eq!(inputs, [8]);
    assert_eq!(pending.readiness().outcome()?, None);
    batch.begin_when(&cpu, FrameBatchPlan::new(0, 0), &pending.readiness())?;
    drop(batch);
    Ok(())
}

#[test]
fn fan_in_reservation_failure_rolls_back_all_earlier_subscriptions() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let first = CompletionPort::new(1)?;
    let full = CompletionPort::new(0)?;
    let mut batch = FrameBatch::<usize>::new(|_| {});
    assert!(matches!(
        batch.begin_after(
            &cpu,
            FrameBatchPlan::new(0, 0),
            &[first.readiness(), full.readiness()]
        ),
        Err(CpuError::ReadinessCapacity)
    ));
    assert!(matches!(
        batch.begin_after(
            &cpu,
            FrameBatchPlan::new(0, 0),
            &[first.readiness(), first.readiness()]
        ),
        Err(CpuError::DuplicateReadiness)
    ));
    batch.begin_when(&cpu, FrameBatchPlan::new(0, 0), &first.readiness())?;
    drop(batch);
    Ok(())
}

#[test]
fn a_join_template_consumes_different_typed_producer_phases() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let gate = CompletionPort::new(1)?;
    let mut producer = gate.producer()?;
    let first_value = Arc::new(AtomicUsize::new(0));
    let second_value = Arc::new(AtomicUsize::new(0));
    let mut first =
        FrameBatch::new(|value: &mut Arc<AtomicUsize>| value.store(2, Ordering::Release));
    first.start(&cpu, &mut vec![Arc::clone(&first_value)])?;
    let mut second = FrameBatch::new(|value: &mut (Arc<AtomicUsize>, usize)| {
        value.0.store(value.1, Ordering::Release)
    });
    second.begin_when(&cpu, FrameBatchPlan::new(1, 0), &gate.readiness())?;
    second.push(&mut Some((Arc::clone(&second_value), 3)))?;
    second.close();
    let mut joined = FrameBatch::new(|value: &mut ([Arc<AtomicUsize>; 2], usize)| {
        value.1 = value.0[0].load(Ordering::Acquire) + value.0[1].load(Ordering::Acquire);
    });
    let mut jobs = vec![([first_value, second_value], 0)];
    joined.start_graph(
        &cpu,
        &FrameGraphTemplate::independent(1),
        &mut jobs,
        &[first.completion()?, second.completion()?],
    )?;
    let result = joined.job(0)?;
    assert_eq!(joined.outcome(&result)?, None);
    producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(joined.with_result(&result, |value| value.1)?, 5);
    joined.reclaim(&mut jobs)?;
    first.reclaim(&mut Vec::new())?;
    second.reclaim(&mut Vec::new())?;
    Ok(())
}

/// Immutable input plans refer to atomic products solely to assert execution order.
struct Work {
    node: usize,
    epoch: usize,
    products: Arc<[AtomicUsize; 4]>,
    output: usize,
}

/// The diamond's join reads both branches; incorrect readiness changes its result.
fn compute(work: &mut Work) {
    let input = match work.node {
        0 => work.epoch,
        1 | 2 => work.products[0].load(Ordering::Acquire) + work.node,
        3 => work.products[1].load(Ordering::Acquire) + work.products[2].load(Ordering::Acquire),
        _ => unreachable!("fixture has four nodes"),
    };
    work.output = input;
    work.products[work.node].store(input, Ordering::Release);
}

#[test]
fn diamond_template_rebinds_generations_without_revalidating_structure()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let template = FrameGraphTemplate::with_dependencies(&[&[], &[0], &[0], &[1, 2]])?;
    let mut batch = FrameBatch::new(compute);
    let products = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
    let mut jobs: Vec<_> = (0..4)
        .map(|node| Work {
            node,
            epoch: 1,
            products: Arc::clone(&products),
            output: 0,
        })
        .collect();
    let mut prior = None;
    for epoch in 1..=100 {
        for job in &mut jobs {
            job.epoch = epoch;
        }
        batch.start_graph(&cpu, &template, &mut jobs, &[])?;
        if let Some(prior) = prior {
            assert!(matches!(batch.outcome(&prior), Err(CpuError::StaleJob)));
        }
        let result = batch.job(3)?;
        assert_eq!(
            batch.with_result(&result, |work| work.output)?,
            2 * epoch + 3
        );
        prior = Some(result);
        batch.reclaim(&mut jobs)?;
    }
    Ok(())
}

#[test]
fn template_validation_and_binding_rejection_do_not_consume_inputs() -> Result<(), Box<dyn Error>> {
    for invalid in [
        &[&[0][..]][..],
        &[&[][..], &[1]],
        &[&[][..], &[0, 0]],
        &[&[4][..]],
    ] {
        assert!(matches!(
            FrameGraphTemplate::with_dependencies(invalid),
            Err(CpuError::InvalidGraph)
        ));
    }
    let cpu = cpu()?;
    let mut batch = FrameBatch::new(|value| *value += 1);
    let template = FrameGraphTemplate::independent(2);
    let mut jobs = vec![1];
    assert!(matches!(
        batch.start_graph(&cpu, &template, &mut jobs, &[]),
        Err(CpuError::GraphInputCount)
    ));
    assert_eq!(jobs, [1]);
    jobs.push(2);
    batch.start_graph(&cpu, &template, &mut jobs, &[])?;
    drop(template);
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs, [2, 3]);
    Ok(())
}

#[test]
fn a_template_failure_suppresses_only_its_descendants() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let template = FrameGraphTemplate::with_dependencies(&[&[], &[0], &[], &[1, 2]])?;
    let mut batch = FrameBatch::with_outcome(|value| {
        if *value == 0 {
            JobOutcome::Failed
        } else {
            *value += 10;
            JobOutcome::Succeeded
        }
    });
    let mut jobs = vec![0, 1, 2, 3];
    batch.start_graph(&cpu, &template, &mut jobs, &[])?;
    assert!(matches!(batch.reclaim(&mut jobs), Err(CpuError::JobFailed)));
    assert_eq!(jobs, [0, 1, 12, 3]);
    Ok(())
}
