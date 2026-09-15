//! Observable dispatch order, inherited urgency and bounded flexible service.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan,
    FrameGraphTemplate, FramePriority, JobOutcome,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
};

/// One lane makes every asserted queue ordering independent of timing estimates.
fn cpu() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(16).ok_or(CpuError::InvalidJob)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))
}

/// A controlled service operation holds dispatch while the complete test graph enters.
fn hold(
    cpu: &CpuExecutor,
) -> Result<(mpsc::SyncSender<()>, solarity_cpu::CpuTask<()>), Box<dyn Error>> {
    let (started, observed) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let task = cpu.try_submit(move || {
        let _sent = started.send(());
        let _released = released.recv();
    })?;
    observed.recv()?;
    Ok((release, task))
}

/// Records only kernel execution; scheduler locks must not be held by this callback.
fn record(input: &mut (Arc<Mutex<Vec<usize>>>, usize)) -> JobOutcome {
    let Ok(mut order) = input.0.lock() else {
        return JobOutcome::Failed;
    };
    order.push(input.1);
    JobOutcome::Succeeded
}

#[test]
fn a_critical_consumer_promotes_its_queued_prerequisite_ahead_of_older_pass_work()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let order = Arc::new(Mutex::new(Vec::new()));
    let (release, blocked) = hold(&cpu)?;
    let mut ordinary = FrameBatch::with_outcome(record);
    ordinary.start(&cpu, &mut vec![(Arc::clone(&order), 1)])?;
    let mut source = FrameBatch::with_outcome(record);
    source.start(&cpu, &mut vec![(Arc::clone(&order), 2)])?;
    let mut consumer = FrameBatch::with_outcome(record);
    consumer.start_graph(
        &cpu,
        &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
        &mut vec![(Arc::clone(&order), 3)],
        &[source.completion()?],
    )?;
    release.send(())?;
    blocked.join()?;
    consumer.reclaim(&mut Vec::new())?;
    source.reclaim(&mut Vec::new())?;
    ordinary.reclaim(&mut Vec::new())?;
    assert_eq!(*order.lock().map_err(|_| "order")?, [2, 3, 1]);
    Ok(())
}

/// The first normal kernel blocks under fixture control; subsequent jobs are finite.
struct Work {
    order: Arc<Mutex<Vec<usize>>>,
    value: usize,
    gate: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
}

/// Running work finishes once released; a queued urgent phase may then take its lane.
fn gated(input: &mut Work) -> JobOutcome {
    if let Some((started, release)) = input.gate.take() {
        let _started = started.send(());
        if release.recv().is_err() {
            return JobOutcome::Failed;
        }
    }
    record(&mut (Arc::clone(&input.order), input.value))
}

#[test]
fn a_running_pass_yields_between_kernels_for_new_urgent_work() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let order = Arc::new(Mutex::new(Vec::new()));
    let (started, observed) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let mut ordinary = FrameBatch::with_outcome(gated);
    ordinary.start(
        &cpu,
        &mut vec![
            Work {
                order: Arc::clone(&order),
                value: 1,
                gate: Some((started, released)),
            },
            Work {
                order: Arc::clone(&order),
                value: 2,
                gate: None,
            },
        ],
    )?;
    observed.recv()?;
    let mut urgent = FrameBatch::with_outcome(record);
    urgent.start_graph(
        &cpu,
        &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
        &mut vec![(Arc::clone(&order), 9)],
        &[],
    )?;
    release.send(())?;
    urgent.reclaim(&mut Vec::new())?;
    ordinary.reclaim(&mut Vec::new())?;
    assert_eq!(*order.lock().map_err(|_| "order")?, [1, 9, 2]);
    Ok(())
}

#[test]
fn priority_reaches_an_external_producer_through_pending_phases() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let producer = port.producer()?;
    let mut token = port.readiness();
    let mut phases = Vec::new();
    for _ in 0..8 {
        let mut batch = FrameBatch::<usize>::new(|value| *value += 1);
        batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
        batch.push(&mut Some(0))?;
        batch.close();
        token = batch.completion()?;
        phases.push(batch);
    }
    phases.last().ok_or("tail")?.require_urgent()?;
    // All priority metadata precedes this frame marker, including metadata
    // appended by other promotion records. No polling or timing threshold.
    let mut marker = FrameBatch::new(|input: &mut (solarity_cpu::CompletionProducer, bool)| {
        input.1 = input.0.is_urgent().is_ok_and(|urgent| urgent);
    });
    let mut inputs = vec![(producer, false)];
    marker.start(&cpu, &mut inputs)?;
    marker.reclaim(&mut inputs)?;
    assert!(inputs[0].1);
    inputs[0].0.complete(JobOutcome::Succeeded)?;
    for phase in &mut phases {
        phase.reclaim(&mut Vec::new())?;
    }
    Ok(())
}

#[test]
fn a_rebound_phase_does_not_keep_its_previous_urgency() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut reused = FrameBatch::with_outcome(record);
    reused.start_graph(
        &cpu,
        &FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite),
        &mut vec![(Arc::clone(&order), 9)],
        &[],
    )?;
    reused.reclaim(&mut Vec::new())?;
    order.lock().map_err(|_| "order")?.clear();
    let (release, blocked) = hold(&cpu)?;
    let mut first = FrameBatch::with_outcome(record);
    first.start(&cpu, &mut vec![(Arc::clone(&order), 1)])?;
    reused.start(&cpu, &mut vec![(Arc::clone(&order), 2)])?;
    first.require_urgent()?;
    release.send(())?;
    blocked.join()?;
    first.reclaim(&mut Vec::new())?;
    reused.reclaim(&mut Vec::new())?;
    assert_eq!(*order.lock().map_err(|_| "order")?, [1, 2]);
    Ok(())
}

/// Completion observation avoids promoting the ordinary phase under test.
struct Marker {
    order: Arc<Mutex<Vec<usize>>>,
    value: usize,
    finished: Option<mpsc::SyncSender<()>>,
}

#[test]
fn one_flexible_worker_alternates_background_service_and_frame_kernel_boundaries()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let order = Arc::new(Mutex::new(Vec::new()));
    let (release, blocked) = hold(&cpu)?;
    let mut background = Vec::new();
    for value in 11..=13 {
        let order = Arc::clone(&order);
        background.push(cpu.try_submit(move || record(&mut (order, value)))?);
    }
    let (finished, observed) = mpsc::sync_channel(1);
    let mut batch = FrameBatch::with_outcome(|input: &mut Marker| {
        let outcome = record(&mut (Arc::clone(&input.order), input.value));
        if let Some(finished) = input.finished.take() {
            let _sent = finished.send(());
        }
        outcome
    });
    let mut jobs = (1..=4)
        .map(|value| Marker {
            order: Arc::clone(&order),
            value,
            finished: (value == 4).then(|| finished.clone()),
        })
        .collect();
    batch.start(&cpu, &mut jobs)?;
    release.send(())?;
    observed.recv()?;
    batch.reclaim(&mut jobs)?;
    blocked.join()?;
    for task in background {
        assert_eq!(task.join()?, JobOutcome::Succeeded);
    }
    assert_eq!(
        *order.lock().map_err(|_| "order")?,
        [1, 11, 2, 12, 3, 13, 4]
    );
    Ok(())
}
