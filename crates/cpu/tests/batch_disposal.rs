//! Disposed consumers leave finite owned work accountable to executor shutdown.

use solarity_cpu::{CpuExecutor, CpuPoolConfig, FrameBatch, JobOutcome};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

/// The controlled child stays in flight while the parent disposes its consumer.
struct Child {
    started: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    finished: Arc<AtomicBool>,
}

#[test]
fn disposing_a_batch_on_a_worker_never_waits_for_another_kernel() -> Result<(), Box<dyn Error>> {
    let cpu = Arc::new(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(2).ok_or("worker count")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(4).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?);
    let finished = Arc::new(AtomicBool::new(false));
    let (started, started_rx) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let (disposed, disposed_rx) = mpsc::sync_channel(1);
    let child = Child {
        started,
        release: release_rx,
        finished: Arc::clone(&finished),
    };
    let owner = Arc::clone(&cpu);
    let parent = cpu.try_submit(move || -> Result<(), solarity_cpu::CpuError> {
        let mut batch = FrameBatch::with_outcome(|child: &mut Child| {
            if child.started.send(()).is_err() || child.release.recv().is_err() {
                return JobOutcome::Failed;
            }
            child.finished.store(true, Ordering::Release);
            JobOutcome::Succeeded
        });
        batch.start(&owner, &mut vec![child])?;
        drop(batch);
        let _observed = disposed.send(());
        Ok(())
    })?;
    let started = started_rx.recv_timeout(Duration::from_secs(5));
    let disposed = disposed_rx.recv_timeout(Duration::from_secs(5));
    assert!(!finished.load(Ordering::Acquire));
    // Release before asserting timeouts so a blocking-drop regression also drains.
    let _released = release.send(());
    parent.join()??;
    let mut cpu = Arc::try_unwrap(cpu).map_err(|_| "parent retained executor")?;
    cpu.shutdown()?;
    started?;
    disposed?;
    assert!(finished.load(Ordering::Acquire));
    Ok(())
}
