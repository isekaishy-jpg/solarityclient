//! A shared service handle cannot retain an unsubmitted admission during shutdown.

use solarity_cpu::{CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use std::{cell::Cell, error::Error, num::NonZeroUsize, sync::mpsc};

#[test]
fn service_handle_admits_before_transfer_and_obeys_owner_shutdown() -> Result<(), Box<dyn Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?;
    let handle = cpu.service_handle();
    let entered = Cell::new(false);
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let result = handle.try_submit_prepared(|| {
        entered.set(true);
        |_: &solarity_cpu::JobContext<'_>| 7
    });
    assert!(matches!(result, Err(CpuError::AtCapacity { .. })));
    assert!(!entered.get());
    release.send(())?;
    blocker.join()??;
    let mut inputs = Some(vec![7, 11]);
    let task = handle.clone().try_submit_prepared(|| {
        let inputs = inputs.take();
        move |_: &solarity_cpu::JobContext<'_>| {
            assert!(solarity_cpu::is_worker_thread());
            inputs
        }
    })?;
    assert!(inputs.is_none());
    assert_eq!(task.join()?, Some(vec![7, 11]));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = handle.try_submit_prepared(|| -> fn(&solarity_cpu::JobContext<'_>) {
            panic!("factory failure")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    cpu.shutdown()?;
    drop(cpu);
    assert!(matches!(
        handle.try_submit_prepared(|| {
            entered.set(true);
            |_: &solarity_cpu::JobContext<'_>| 9
        }),
        Err(CpuError::ShuttingDown)
    ));
    assert!(!entered.get());
    Ok(())
}
