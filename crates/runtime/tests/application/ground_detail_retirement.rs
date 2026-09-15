//! CPU-only retirement preserves ownership through saturation and shutdown.

use super::CpuRetirementQueue;
use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig};
use std::num::NonZeroUsize;
use std::sync::mpsc::{Sender, channel};
use std::thread::{self, ThreadId};
use std::time::Duration;

struct DropProbe(Sender<ThreadId>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        let _ = self.0.send(thread::current().id());
    }
}

#[test]
fn detail_retirement_retries_saturation_and_shutdown_waits_for_worker_drops()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let occupied = cpu.try_reserve()?;
    let (sender, receiver) = channel();
    let mut queue = CpuRetirementQueue::new();
    let count = super::RETIREMENTS_PER_STEP * 3 + 1;
    queue.extend((0..count).map(|_| DropProbe(sender.clone())));
    queue.service(&cpu)?;
    assert_eq!(queue.pending.len(), count);
    assert!(
        receiver.try_recv().is_err(),
        "saturation must preserve ownership"
    );
    drop(occupied);
    queue.service(&cpu)?;
    assert!(queue.pending.is_empty());
    cpu.shutdown()?;
    for _ in 0..count {
        assert_ne!(
            receiver.recv_timeout(Duration::from_secs(5))?,
            thread::current().id()
        );
    }
    assert!(
        receiver.try_recv().is_err(),
        "each allocation retires exactly once"
    );
    Ok(())
}

#[test]
fn detail_retirement_keeps_unsubmitted_ownership_when_admission_is_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    cpu.shutdown()?;
    let (sender, receiver) = channel();
    let mut queue = CpuRetirementQueue::new();
    queue.service(&cpu)?;
    queue.extend([DropProbe(sender)]);
    assert!(matches!(queue.service(&cpu), Err(CpuError::ShuttingDown)));
    assert_eq!(queue.pending.len(), 1);
    assert!(receiver.try_recv().is_err());
    drop(queue);
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5))?,
        thread::current().id()
    );
    Ok(())
}

/// A real retirement blocks its first destructor until required service is queued.
struct OrderedDrop {
    index: usize,
    record: Sender<usize>,
    gate: Option<(Sender<()>, std::sync::mpsc::Receiver<()>)>,
}
impl Drop for OrderedDrop {
    fn drop(&mut self) {
        if let Some((started, wait)) = self.gate.take() {
            let _sent = started.send(());
            let _released = wait.recv();
        }
        let _sent = self.record.send(self.index);
    }
}

#[test]
fn required_loading_gets_a_turn_before_the_retirement_backlog_finishes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(2).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?;
    let (record, order) = channel();
    let (started, observed) = channel();
    let (release, wait) = channel();
    let mut queue = CpuRetirementQueue::new();
    queue.extend([OrderedDrop {
        index: 0,
        record: record.clone(),
        gate: Some((started, wait)),
    }]);
    queue.extend((1..64).map(|index| OrderedDrop {
        index,
        record: record.clone(),
        gate: None,
    }));
    queue.service(&cpu)?;
    observed.recv_timeout(Duration::from_secs(5))?;
    let required = cpu.try_submit(move || record.send(1000))?;
    release.send(())?;
    cpu.shutdown()?;
    required.join()??;
    let actual = (0..65)
        .map(|_| order.recv_timeout(Duration::from_secs(5)))
        .collect::<Result<Vec<_>, _>>()?;
    let service = actual
        .iter()
        .position(|value| *value == 1000)
        .ok_or("required service")?;
    assert!(
        service > 0 && service < 64,
        "loading must run during cleanup"
    );
    assert!(service <= super::RETIREMENTS_PER_STEP);
    assert_eq!(
        actual
            .into_iter()
            .filter(|value| *value != 1000)
            .collect::<Vec<_>>(),
        (0..64).collect::<Vec<_>>()
    );
    Ok(())
}
