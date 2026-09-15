//! A phase notification follows its last metadata operation, even with no kernels.

use solarity_cpu::{
    CoordinatorNotifier, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan, FramePriority,
    JobOutcome, ReadyToken,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

/// Records only notifications that can already observe the phase's terminal state.
#[derive(Default)]
struct Notifier {
    phase: Mutex<Option<ReadyToken>>,
    ready_signals: AtomicUsize,
}
impl CoordinatorNotifier for Notifier {
    fn notify(&self) {
        let Ok(phase) = self.phase.lock() else {
            return;
        };
        if phase
            .as_ref()
            .is_some_and(|token| matches!(token.outcome(), Ok(Some(JobOutcome::Succeeded))))
        {
            self.ready_signals.fetch_add(1, Ordering::Release);
        }
    }
}

#[test]
fn an_empty_promoted_phase_notifies_after_metadata_finishes() -> Result<(), Box<dyn Error>> {
    let notifier = Arc::new(Notifier::default());
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::new(4).ok_or("capacity")?),
        notifier.clone(),
    )?;
    let (started, observed) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let blocked = cpu.try_submit(move || {
        let _sent = started.send(());
        let _released = released.recv();
    })?;
    observed.recv()?;
    let mut batch = FrameBatch::<usize>::new(|_| {});
    batch.begin(
        &cpu,
        FrameBatchPlan::new(0, 0).with_priority(FramePriority::Prerequisite),
    )?;
    *notifier.phase.lock().map_err(|_| "phase")? = Some(batch.completion()?);
    batch.close();
    release.send(())?;
    cpu.shutdown()?;
    blocked.join()?;
    batch.reclaim(&mut Vec::new())?;
    assert_eq!(notifier.ready_signals.load(Ordering::Acquire), 1);
    Ok(())
}
