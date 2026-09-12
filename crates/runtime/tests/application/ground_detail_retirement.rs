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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    let occupied = cpu.try_reserve()?;
    let (sender, receiver) = channel();
    let mut queue = CpuRetirementQueue::new();
    queue.extend((0..3).map(|_| DropProbe(sender.clone())));
    queue.service(&cpu)?;
    assert_eq!(queue.pending.len(), 3);
    assert!(
        receiver.try_recv().is_err(),
        "saturation must preserve ownership"
    );
    drop(occupied);
    queue.service(&cpu)?;
    assert!(queue.pending.is_empty());
    cpu.shutdown()?;
    for _ in 0..3 {
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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
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
