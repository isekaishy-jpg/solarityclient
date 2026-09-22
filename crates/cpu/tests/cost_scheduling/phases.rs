//! Competing phases expose ready-cost changes and finite worker turns.

use super::{cpu, hold, record};
use solarity_cpu::{
    CpuError, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch, FrameBatchPlan,
    FrameGraphTemplate, FramePriority, JobCost,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

/// Three policy classes with known hints; no wall-clock speed assertion is involved.
fn cost(micros: u64) -> JobCost {
    JobCost::measured(Duration::from_micros(micros))
}

#[test]
fn phases_order_by_urgency_then_cost_with_fifo_ties() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut phases = Vec::new();
    for (id, micros, priority) in [
        (0, 10, FramePriority::Pass),
        (1, 400, FramePriority::Pass),
        (2, 100, FramePriority::Pass),
        (3, 10, FramePriority::Prerequisite),
        (4, 400, FramePriority::Prerequisite),
        (5, 500, FramePriority::Prerequisite),
    ] {
        let mut batch = FrameBatch::new(record);
        batch.start_costed_graph(
            &cpu,
            &FrameGraphTemplate::independent(1).with_priority(priority),
            &mut vec![(send.clone(), id)],
            &[],
            &[cost(micros)],
        )?;
        phases.push(batch);
    }
    release.send(())?;
    blocker.join()?;
    let order = (0..6)
        .map(|_| receive.recv())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(order, [4, 5, 3, 1, 2, 0]);
    for (id, mut batch) in phases.into_iter().enumerate() {
        let mut jobs = Vec::new();
        batch.reclaim(&mut jobs)?;
        assert_eq!(jobs[0].1, id);
    }
    Ok(())
}

#[test]
fn incremental_heavy_input_promotes_existing_runners_then_yields_when_cost_drops()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut earlier = FrameBatch::new(record);
    earlier.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(1),
        &mut vec![(send.clone(), 0)],
        &[],
        &[cost(100)],
    )?;
    let mut incremental = FrameBatch::new(record);
    incremental.begin(&cpu, FrameBatchPlan::new(2, 0))?;
    incremental.push_with_cost(&mut Some((send.clone(), 1)), cost(10))?;
    // This phase's only runner is already queued as cheap. Updating a new
    // runner alone would leave the heavy job buried behind the earlier phase.
    incremental.push_with_cost(&mut Some((send, 2)), cost(500))?;
    incremental.close();
    release.send(())?;
    blocker.join()?;
    assert_eq!(
        [receive.recv()?, receive.recv()?, receive.recv()?],
        [2, 0, 1]
    );
    earlier.reclaim(&mut Vec::new())?;
    let mut jobs = Vec::new();
    incremental.reclaim(&mut jobs)?;
    assert_eq!(jobs.iter().map(|job| job.1).collect::<Vec<_>>(), [1, 2]);
    Ok(())
}

#[test]
fn cancelling_the_heavy_head_repairs_queued_cost_before_execution() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut changed = FrameBatch::new(record);
    changed.begin(&cpu, FrameBatchPlan::new(2, 0))?;
    let cancelled = changed.push_with_cost(&mut Some((send.clone(), 0)), cost(500))?;
    changed.push_with_cost(&mut Some((send.clone(), 1)), cost(10))?;
    changed.cancel(&cancelled)?;
    changed.close();
    let mut medium = FrameBatch::new(record);
    medium.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(1),
        &mut vec![(send, 2)],
        &[],
        &[cost(100)],
    )?;
    release.send(())?;
    blocker.join()?;
    assert_eq!([receive.recv()?, receive.recv()?], [2, 1]);
    let mut jobs = Vec::new();
    assert!(matches!(
        changed.reclaim(&mut jobs),
        Err(CpuError::JobCancelled)
    ));
    assert_eq!(jobs.iter().map(|job| job.1).collect::<Vec<_>>(), [0, 1]);
    medium.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn equal_cost_phases_receive_fifo_kernel_turns() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut first = FrameBatch::new(record);
    let mut second = FrameBatch::new(record);
    first.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(2),
        &mut vec![(send.clone(), 0), (send.clone(), 1)],
        &[],
        &[cost(100); 2],
    )?;
    second.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(2),
        &mut vec![(send.clone(), 2), (send, 3)],
        &[],
        &[cost(100); 2],
    )?;
    release.send(())?;
    blocker.join()?;
    assert_eq!(
        [
            receive.recv()?,
            receive.recv()?,
            receive.recv()?,
            receive.recv()?
        ],
        [0, 2, 1, 3]
    );
    first.reclaim(&mut Vec::new())?;
    second.reclaim(&mut Vec::new())?;
    Ok(())
}

/// A kernel remains indivisible; only its return releases the lane for another phase.
struct HeldJob {
    output: mpsc::Sender<usize>,
    id: usize,
    gate: Option<mpsc::Receiver<()>>,
}

/// Announcing start lets main add competing work while the first kernel owns its lane.
fn held(job: &mut HeldJob) {
    let _sent = job.output.send(job.id);
    if let Some(gate) = job.gate.take() {
        let _released = gate.recv();
    }
}

#[test]
fn released_heavy_successor_precedes_another_phase_without_bypassing_its_parent()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (send, receive) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut graph = FrameBatch::new(held);
    let template = FrameGraphTemplate::with_dependencies(cpu.storage(), &[&[], &[], &[0]])?;
    graph.start_costed_graph(
        &cpu,
        &template,
        &mut vec![
            HeldJob {
                output: send.clone(),
                id: 0,
                gate: Some(gate),
            },
            HeldJob {
                output: send.clone(),
                id: 1,
                gate: None,
            },
            HeldJob {
                output: send.clone(),
                id: 2,
                gate: None,
            },
        ],
        &[],
        &[cost(100), cost(10), cost(500)],
    )?;
    assert_eq!(receive.recv()?, 0);
    let mut medium = FrameBatch::new(record);
    medium.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(1),
        &mut vec![(send, 3)],
        &[],
        &[cost(100)],
    )?;
    assert!(
        receive.try_recv().is_err(),
        "running kernel still owns the only lane"
    );
    release.send(())?;
    assert_eq!(
        [receive.recv()?, receive.recv()?, receive.recv()?],
        [2, 3, 1]
    );
    graph.reclaim(&mut Vec::new())?;
    medium.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn required_service_still_alternates_with_heavy_frame_work_on_one_worker()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let (release, blocker) = hold(&cpu)?;
    let (send, receive) = mpsc::channel();
    let mut frame = FrameBatch::new(record);
    frame.start_costed_graph(
        &cpu,
        &FrameGraphTemplate::independent(3),
        &mut (0..3).map(|id| (send.clone(), id)).collect::<Vec<_>>(),
        &[],
        &[cost(500); 3],
    )?;
    let mut service = Vec::new();
    for id in [3, 4] {
        let send = send.clone();
        service.push(cpu.try_submit(move || {
            let _sent = send.send(id);
        })?);
    }
    release.send(())?;
    blocker.join()?;
    let order = (0..5)
        .map(|_| receive.recv())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(order, [0, 3, 1, 4, 2]);
    for task in service {
        task.join()?;
    }
    frame.reclaim(&mut Vec::new())?;
    Ok(())
}

/// Concurrent admission/claims exercise raise, decrease and epoch reuse races;
/// each result must retain its original identity and execute exactly once.
#[test]
fn concurrent_cost_changes_preserve_every_input_across_reused_phases() -> Result<(), Box<dyn Error>>
{
    for workers in [2, 4] {
        let cpu = CpuExecutor::new(CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::new(workers).ok_or("workers")?;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::new(8).ok_or("capacity")?,
            CpuStoragePlan::new(64 << 20, 64 << 20, 0),
        ))?;
        let mut phases: Vec<_> = (0..3)
            .map(|_| FrameBatch::new(|job: &mut (usize, usize)| job.1 += 1))
            .collect();
        let mut returned: [Vec<(usize, usize)>; 3] = std::array::from_fn(|_| Vec::new());
        for epoch in 0..16 {
            for phase in &mut phases {
                phase.begin(&cpu, FrameBatchPlan::new(64, 0))?;
            }
            for index in 0..64 {
                for (phase_index, phase) in phases.iter_mut().enumerate() {
                    let id = epoch * 192 + phase_index * 64 + index;
                    phase.push_with_cost(&mut Some((id, 0)), cost([10, 100, 500][index % 3]))?;
                }
            }
            for phase in &mut phases {
                phase.close();
            }
            for (phase_index, phase) in phases.iter_mut().enumerate() {
                returned[phase_index].clear();
                phase.reclaim(&mut returned[phase_index])?;
                let expected: Vec<_> = (0..64)
                    .map(|index| (epoch * 192 + phase_index * 64 + index, 1))
                    .collect();
                assert_eq!(returned[phase_index], expected);
            }
        }
    }
    Ok(())
}
