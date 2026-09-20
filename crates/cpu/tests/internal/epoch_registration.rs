//! Independent epoch admission must not borrow another batch's metadata lock.

use super::{FrameBatch, FrameBatchPlan};
use crate::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

/// Hold a real batch lock while another thread admits an independent epoch. The
/// timeout detects the old cross-batch wait; all gates release before asserting
/// so the regression fails without leaving a blocked test worker behind.
#[test]
fn registration_progresses_while_an_unrelated_batch_is_locked() -> Result<(), Box<dyn Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            crate::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 0),
    ))?;
    let mut first = FrameBatch::<u64>::new(|_| {});
    first.begin(&cpu, FrameBatchPlan::new(0, 0))?;
    let core = Arc::clone(&first.core);
    let (locked, lock_observed) = mpsc::channel();
    let (release, release_observed) = mpsc::channel::<()>();
    let (admitted, admission_observed) = mpsc::channel();
    let result = thread::scope(|scope| {
        let holder = scope.spawn(move || {
            let guard = core.lock();
            let _sent = locked.send(());
            let _released = release_observed.recv();
            drop(guard);
        });
        let locked = lock_observed.recv_timeout(Duration::from_secs(5));
        let cpu_ref = &cpu;
        let contender = scope.spawn(move || {
            let mut second = FrameBatch::<u64>::new(|_| {});
            let result = second.begin(cpu_ref, FrameBatchPlan::new(0, 0));
            let _sent = admitted.send(result);
        });
        let admitted = admission_observed.recv_timeout(Duration::from_secs(5));
        drop(release);
        let holder = holder.join();
        let contender = contender.join();
        (locked, admitted, holder, contender)
    });
    first.close();
    cpu.shutdown()?;
    result.0?;
    result.2.map_err(|_| "lock holder panicked")?;
    result.3.map_err(|_| "admission thread panicked")?;
    result
        .1
        .map_err(|_| "independent admission waited for another batch lock")??;
    Ok(())
}
