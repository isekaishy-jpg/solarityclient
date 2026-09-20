//! Bounded dependency execution, late registration and terminal ownership.

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan, JobOutcome};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

/// The fixture owns every blocking gate; production dependencies never block workers.
struct Job {
    state: Arc<AtomicUsize>,
    bit: usize,
    needs: usize,
    wait: Option<Receiver<()>>,
    entered: Option<SyncSender<()>>,
    outcome: JobOutcome,
    ran: bool,
}
impl Job {
    /// Makes one independent mutable input with an explicit shared result bank.
    fn new(state: &Arc<AtomicUsize>, bit: usize, needs: usize) -> Self {
        Self {
            state: Arc::clone(state),
            bit,
            needs,
            wait: None,
            entered: None,
            outcome: JobOutcome::Succeeded,
            ran: false,
        }
    }
}
/// Assertions verify visibility of every predecessor before executing its consumer.
fn execute(job: &mut Job) -> JobOutcome {
    job.ran = true;
    if let Some(entered) = &job.entered {
        assert!(entered.send(()).is_ok());
    }
    if let Some(wait) = &job.wait {
        assert!(wait.recv().is_ok());
    }
    assert_eq!(job.state.load(Ordering::Acquire) & job.needs, job.needs);
    job.state.fetch_or(job.bit, Ordering::Release);
    if job.outcome == JobOutcome::Panicked {
        panic!("fixture operation failure");
    }
    job.outcome
}
/// One admitted frame graph can occupy all workers without an auxiliary pool.
fn cpu(workers: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(workers).ok_or("workers")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

#[test]
fn completed_parent_releases_parallel_children_and_join_without_main_polling()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu(3)?;
    let state = Arc::new(AtomicUsize::new(0));
    let mut graph = FrameBatch::with_outcome(execute);
    graph.begin(&cpu, FrameBatchPlan::new(4, 4))?;
    let (release_root, gate) = mpsc::sync_channel(1);
    let mut root = Job::new(&state, 1, 0);
    root.wait = Some(gate);
    let root = graph.push(&mut Some(root))?;
    let (entered, observed) = mpsc::sync_channel(2);
    let (release_a, gate_a) = mpsc::sync_channel(1);
    let (release_b, gate_b) = mpsc::sync_channel(1);
    let mut a = Job::new(&state, 2, 1);
    a.wait = Some(gate_a);
    a.entered = Some(entered.clone());
    let a = graph.push_after(&mut Some(a), std::slice::from_ref(&root))?;
    let mut b = Job::new(&state, 4, 1);
    b.wait = Some(gate_b);
    b.entered = Some(entered);
    let b = graph.push_after(&mut Some(b), &[root])?;
    let join = graph.push_after(&mut Some(Job::new(&state, 8, 7)), &[a, b])?;
    assert_eq!(graph.try_with_result(&join, |job| job.ran)?, None);
    graph.close();
    release_root.send(())?;
    // Observe both executing children before releasing either. Main has called
    // no scheduler poll/consume since releasing their parent.
    let first = observed.recv_timeout(Duration::from_secs(5));
    let second = observed.recv_timeout(Duration::from_secs(5));
    release_a.send(())?;
    release_b.send(())?;
    first?;
    second?;
    assert!(graph.with_result(&join, |job| job.ran)?);
    assert_eq!(state.load(Ordering::Acquire), 15);
    let mut jobs = Vec::new();
    graph.reclaim(&mut jobs)?;
    assert_eq!(jobs.len(), 4);
    Ok(())
}

#[test]
fn late_registration_reuse_and_foreign_handles_cannot_address_another_epoch()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu(1)?;
    let mut graph = FrameBatch::new(|value| *value += 1usize);
    let mut other = FrameBatch::new(|value| *value += 1usize);
    other.begin(&cpu, FrameBatchPlan::new(1, 0))?;
    let foreign = other.push(&mut Some(0))?;
    let mut returned = Vec::new();
    let mut old = None;
    for _ in 0..100 {
        graph.begin(&cpu, FrameBatchPlan::new(2, 1))?;
        let parent = graph.push(&mut Some(0))?;
        graph.with_result(&parent, |_| ())?;
        let mut input = Some(10);
        assert!(matches!(
            graph.push_after(&mut input, std::slice::from_ref(&foreign)),
            Err(CpuError::StaleJob)
        ));
        assert_eq!(input, Some(10));
        if let Some(stale) = &old {
            assert!(matches!(graph.outcome(stale), Err(CpuError::StaleJob)));
            assert!(matches!(
                graph.push_after(&mut input, std::slice::from_ref(stale)),
                Err(CpuError::StaleJob)
            ));
        }
        let child = graph.push_after(&mut input, &[parent])?;
        assert_eq!(graph.with_result(&child, |value| *value)?, 11);
        old = Some(child);
        returned.clear();
        graph.reclaim(&mut returned)?;
        assert_eq!(returned, [1, 11]);
    }
    other.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn capacity_and_failed_reservation_preserve_inputs_and_admission() -> Result<(), Box<dyn Error>> {
    let cpu = cpu(1)?;
    let mut graph = FrameBatch::new(|value| *value += 1usize);
    assert!(matches!(
        graph.begin(&cpu, FrameBatchPlan::new(usize::MAX, 0)),
        Err(CpuError::StorageSizeOverflow)
    ));
    graph.begin(&cpu, FrameBatchPlan::new(3, 1))?;
    let parent = graph.push(&mut Some(0))?;
    graph.with_result(&parent, |_| ())?;
    graph.push_after(&mut Some(1), std::slice::from_ref(&parent))?;
    let mut input = Some(99);
    assert!(matches!(
        graph.push_after(&mut input, &[parent]),
        Err(CpuError::BatchCapacity)
    ));
    assert_eq!(input, Some(99));
    graph.push(&mut input)?;
    input = Some(100);
    assert!(matches!(
        graph.push(&mut input),
        Err(CpuError::BatchCapacity)
    ));
    assert_eq!(input, Some(100));
    graph.close();
    assert!(matches!(graph.push(&mut input), Err(CpuError::BatchClosed)));
    graph.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn failure_and_panic_skip_successors_but_return_every_owned_input() -> Result<(), Box<dyn Error>> {
    for outcome in [JobOutcome::Failed, JobOutcome::Panicked] {
        let cpu = cpu(1)?;
        let state = Arc::new(AtomicUsize::new(0));
        let mut graph = FrameBatch::with_outcome(execute);
        graph.begin(&cpu, FrameBatchPlan::new(3, 2))?;
        let mut root = Job::new(&state, 1, 0);
        root.outcome = outcome;
        let root = graph.push(&mut Some(root))?;
        let child = graph.push_after(&mut Some(Job::new(&state, 2, 1)), &[root])?;
        let last = graph.push_after(&mut Some(Job::new(&state, 4, 3)), &[child])?;
        assert!(matches!(
            graph.with_result(&last, |_| ()),
            Err(CpuError::DependencyFailed)
        ));
        let mut returned = Vec::new();
        let error = graph.reclaim(&mut returned);
        assert!(matches!(
            (outcome, error),
            (JobOutcome::Failed, Err(CpuError::JobFailed))
                | (JobOutcome::Panicked, Err(CpuError::TaskPanicked))
        ));
        assert_eq!(returned.len(), 3);
        assert!(returned[0].ran);
        assert!(!returned[1].ran && !returned[2].ran);
    }
    Ok(())
}

#[test]
fn cancellation_retains_running_and_waiting_state_until_its_producers_finish()
-> Result<(), Box<dyn Error>> {
    for cancel_running in [false, true] {
        let cpu = cpu(1)?;
        let state = Arc::new(AtomicUsize::new(0));
        let mut graph = FrameBatch::with_outcome(execute);
        graph.begin(&cpu, FrameBatchPlan::new(3, 2))?;
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let mut root = Job::new(&state, 1, 0);
        root.wait = Some(gate);
        root.entered = Some(entered);
        let root = graph.push(&mut Some(root))?;
        observed.recv_timeout(Duration::from_secs(5))?;
        let child = graph.push_after(
            &mut Some(Job::new(&state, 2, 1)),
            std::slice::from_ref(&root),
        )?;
        let last = graph.push_after(
            &mut Some(Job::new(&state, 4, 3)),
            std::slice::from_ref(&child),
        )?;
        if cancel_running {
            graph.cancel(&root)?;
            assert_eq!(graph.outcome(&root)?, None);
        } else {
            graph.cancel(&child)?;
            graph.cancel(&child)?;
            assert_eq!(graph.outcome(&last)?, Some(JobOutcome::DependencyFailed));
        }
        release.send(())?;
        assert!(matches!(
            graph.with_result(&last, |_| ()),
            Err(CpuError::DependencyFailed)
        ));
        let mut returned = Vec::new();
        assert!(matches!(
            graph.reclaim(&mut returned),
            Err(CpuError::JobCancelled)
        ));
        assert!(returned[0].ran && !returned[1].ran && !returned[2].ran);
    }
    Ok(())
}
