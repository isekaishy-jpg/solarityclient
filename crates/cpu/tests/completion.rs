//! Runtime notification follows durable CPU result publication.

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use solarity_cpu::{CoordinatorNotifier, CpuExecutor, CpuPoolConfig, FrameBatch};

/// Bounded, non-blocking stand-in for the runtime's native signal owner.
#[derive(Default)]
struct Notifier(AtomicUsize);
impl CoordinatorNotifier for Notifier {
    fn notify(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

#[test]
fn background_and_frame_outputs_notify_after_completing_and_survive_shutdown()
-> Result<(), Box<dyn Error>> {
    let notifier = Arc::new(Notifier::default());
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            NonZeroUsize::new(2).ok_or("workers")?,
            NonZeroUsize::MIN,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        notifier.clone(),
    )?;
    let background = cpu.try_submit(|| 42)?;
    let mut jobs = vec![1, 2];
    let mut batch = FrameBatch::new(|value| *value += 1);
    batch.start(&cpu, &mut jobs)?;
    cpu.shutdown()?;
    assert!(background.is_finished());
    assert_eq!(background.join()?, 42);
    assert_eq!(batch.with_result(&batch.job(1)?, |value| *value)?, 3);
    batch.reclaim(&mut jobs)?;
    assert_eq!(jobs, [2, 3]);
    // Two job results, the phase's later durable readiness, and the background result.
    assert_eq!(notifier.0.load(Ordering::Acquire), 4);
    Ok(())
}
