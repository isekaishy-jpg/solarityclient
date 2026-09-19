//! Controlled queues prove cost policy without measuring wall-clock speed.

#[path = "cost_scheduling/phases.rs"]
mod phases;

use solarity_cpu::{
    CostCalibration, CpuError, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch,
    FrameBatchPlan, FrameGraphTemplate, FramePriority, JobCost, JobOutcome,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

/// One held lane admits the entire fixture before any ordering decision.
fn cpu() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(16).ok_or(CpuError::InvalidJob)?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))
}

/// A test-controlled service call parks the worker without timing assumptions.
fn hold(
    cpu: &CpuExecutor,
) -> Result<(mpsc::Sender<()>, solarity_cpu::CpuTask<()>), Box<dyn Error>> {
    let (started, observed) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let task = cpu.try_submit(move || {
        let _sent = started.send(());
        let _released = released.recv();
    })?;
    observed.recv()?;
    Ok((release, task))
}

/// Records execution separately from the scheduler's stable output slots.
fn record(job: &mut (mpsc::Sender<usize>, usize)) {
    let _sent = job.0.send(job.1);
}

#[test]
fn expensive_ready_jobs_start_first_with_fifo_ties_and_ordered_reclamation()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut batch = FrameBatch::new(record);
    let mut jobs = (0..6).map(|index| (send.clone(), index)).collect();
    let costs = [10, 400, 100, 300, 5, 0].map(|micros| {
        if micros == 0 {
            JobCost::default()
        } else {
            JobCost::measured(Duration::from_micros(micros))
        }
    });
    batch.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(6),
        &mut jobs,
        &[],
        &costs,
    )?;
    release.send(())?;
    blocker.join()?;
    let order = (0..6)
        .map(|_| receive.recv())
        .collect::<Result<Vec<_>, _>>()?;
    batch.reclaim(&mut jobs)?;
    assert_eq!(order, [1, 3, 2, 5, 0, 4]);
    assert_eq!(
        jobs.iter().map(|job| job.1).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
    Ok(())
}

#[test]
fn cost_never_bypasses_dependencies_and_newly_ready_heavy_work_precedes_small_work()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut batch = FrameBatch::new(record);
    let mut jobs = (0..5).map(|index| (send.clone(), index)).collect();
    let template =
        FrameGraphTemplate::with_dependencies(cpu.storage(), &[&[], &[], &[0], &[2], &[]])?;
    let costs =
        [10, 100, 400, 400, 10].map(|micros| JobCost::measured(Duration::from_micros(micros)));
    batch.start_costed_graph(&cpu, &template, &mut jobs, &[], &costs)?;
    release.send(())?;
    blocker.join()?;
    let order = (0..5)
        .map(|_| receive.recv())
        .collect::<Result<Vec<_>, _>>()?;
    batch.reclaim(&mut jobs)?;
    assert_eq!(order, [1, 0, 2, 3, 4]);
    Ok(())
}

#[test]
fn cancelled_cost_bins_release_successors_and_reuse_without_stale_links()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut batch = FrameBatch::new(record);
    let (send, receive) = mpsc::channel();
    for _ in 0..3 {
        let (release, blocker) = hold(&cpu)?;
        batch.begin(&cpu, FrameBatchPlan::new(4, 1))?;
        let heavy = JobCost::measured(Duration::from_millis(1));
        let root = batch.push_with_cost(&mut Some((send.clone(), 0)), heavy)?;
        let child = batch.push_after_with_cost(
            &mut Some((send.clone(), 1)),
            std::slice::from_ref(&root),
            heavy,
        )?;
        batch.push_with_cost(&mut Some((send.clone(), 2)), heavy)?;
        batch.push_with_cost(
            &mut Some((send.clone(), 3)),
            JobCost::measured(Duration::from_micros(1)),
        )?;
        batch.cancel(&root)?;
        batch.close();
        release.send(())?;
        blocker.join()?;
        assert_eq!([receive.recv()?, receive.recv()?], [2, 3]);
        batch.wait_until_finished()?;
        assert_eq!(batch.outcome(&child)?, Some(JobOutcome::DependencyFailed));
        let mut jobs = Vec::new();
        assert!(matches!(
            batch.reclaim(&mut jobs),
            Err(CpuError::JobCancelled)
        ));
        assert_eq!(
            jobs.iter().map(|job| job.1).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }
    Ok(())
}

#[test]
fn prerequisite_priority_precedes_an_expensive_ordinary_phase() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut ordinary = FrameBatch::new(record);
    ordinary.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(1),
        &mut vec![(send.clone(), 0)],
        &[],
        &[JobCost::measured(Duration::from_millis(5))],
    )?;
    let mut critical = FrameBatch::new(record);
    critical.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
        &mut vec![(send, 1)],
        &[],
        &[JobCost::measured(Duration::from_micros(1))],
    )?;
    release.send(())?;
    blocker.join()?;
    assert_eq!([receive.recv()?, receive.recv()?], [1, 0]);
    critical.reclaim(&mut Vec::new())?;
    ordinary.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn rejected_cost_count_preserves_inputs_and_does_not_open_an_epoch() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut batch = FrameBatch::new(|value: &mut usize| *value += 1);
    let mut jobs = vec![1, 2];
    assert!(matches!(
        batch.start_costed_graph(
            &cpu,
            &FrameGraphTemplate::independent(2),
            &mut jobs,
            &[],
            &[JobCost::default()]
        ),
        Err(CpuError::GraphInputCount)
    ));
    assert_eq!(jobs, [1, 2]);
    batch.start(&cpu, &mut jobs)?;
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs, [2, 3]);
    Ok(())
}

#[test]
fn calibration_uses_controlled_durations_and_saturates_large_counts() {
    let mut calibration = CostCalibration::default();
    assert_eq!(calibration.estimate(100).duration(), None);
    calibration.observe(0, Duration::MAX);
    assert_eq!(calibration.units_for(Duration::from_micros(100)), None);
    calibration.observe(10, Duration::from_micros(100));
    assert_eq!(
        calibration.estimate(20).duration(),
        Some(Duration::from_micros(200))
    );
    assert_eq!(calibration.units_for(Duration::from_micros(100)), Some(10));
    calibration.observe(10, Duration::from_micros(180));
    assert_eq!(
        calibration.estimate(20).duration(),
        Some(Duration::from_micros(220))
    );
    calibration.observe(1, Duration::MAX);
    assert_eq!(
        calibration.estimate(usize::MAX).duration(),
        Some(Duration::from_nanos(u64::MAX))
    );
    assert_eq!(calibration.units_for(Duration::ZERO), Some(1));
}

#[test]
fn warmed_calibration_reads_no_clock_for_63_of_64_jobs() {
    let mut calibration = CostCalibration::default();
    let sampled = (0..128)
        .filter(|_| calibration.prepare(1).start().is_some())
        .count();
    assert_eq!(sampled, 9);
    assert!(calibration.prepare(0).start().is_none());
}
