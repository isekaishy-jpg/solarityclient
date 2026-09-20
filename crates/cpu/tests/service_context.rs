//! Service withdrawal preserves producer ownership, admission and continuation identity.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass,
    CpuStorageKind, CpuStoragePlan,
};
use std::{error::Error, num::NonZeroUsize, ops::ControlFlow, sync::mpsc, time::Duration};

/// One worker and controlled gates avoid assumptions about OS scheduling.
fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

/// Cancellation changes demand, not identity or ownership of the returned state.
#[test]
fn cancelled_service_returns_its_inputs_with_the_same_identity_across_steps()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu()?;
    let (entered, observed) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let mut value = Some(vec![7, 11, 13]);
    let mut identity = None;
    let task = cpu
        .try_reserve()?
        .submit_steps_with_context(move |context| {
            if let Some(first) = identity {
                assert_eq!(first, context.identity());
                assert!(context.is_cancelled());
                ControlFlow::Break(value.take())
            } else {
                identity = Some(context.identity());
                assert!(!context.is_cancelled());
                let _sent = entered.send(());
                let _released = wait.recv_timeout(Duration::from_secs(5));
                ControlFlow::Continue(())
            }
        });
    let started = observed.recv_timeout(Duration::from_secs(5));
    task.cancel();
    release.send(())?;
    started?;
    assert_eq!(task.join()?, Some(vec![7, 11, 13]));
    cpu.shutdown()?;
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}

/// Dropping demand never drops required cleanup, even when it sees withdrawal.
#[test]
fn dropped_handles_request_withdrawal_but_shutdown_drains_contextual_cleanup()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu()?;
    let (entered, observed) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || {
        let _sent = entered.send(());
        let _released = wait.recv_timeout(Duration::from_secs(5));
    })?;
    observed.recv_timeout(Duration::from_secs(5))?;
    let (record, results) = mpsc::channel();
    let mut remaining = vec![1, 2, 3];
    let task = cpu
        .try_reserve_for(CpuService::Retirement)?
        .submit_steps_with_context(move |context| {
            assert!(context.is_cancelled());
            match remaining.pop() {
                Some(value) => {
                    let _sent = record.send(value);
                    ControlFlow::Continue(())
                }
                None => ControlFlow::Break(()),
            }
        });
    drop(task);
    release.send(())?;
    blocker.join()?;
    cpu.shutdown()?;
    assert_eq!(results.try_iter().collect::<Vec<_>>(), [3, 2, 1]);
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}

/// Refused metadata leaves both caller inputs and task capacity available for retry.
#[test]
fn service_metadata_is_admitted_before_transfer_and_released_with_unused_permits()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let class = CpuStorageClass::Required;
    let baseline = cpu.storage().snapshot().used(class);
    let permit = cpu.try_reserve()?;
    assert!(cpu.storage().snapshot().used(class) > baseline);
    drop(permit);
    assert_eq!(cpu.storage().snapshot().used(class), baseline);
    let remaining = cpu.storage().snapshot().limit(class) - baseline;
    let pressure = cpu
        .storage()
        .reserve(class, CpuStorageKind::Result, remaining)?;
    assert!(matches!(
        cpu.try_reserve(),
        Err(CpuError::StorageAtCapacity { .. })
    ));
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    drop(pressure);
    let input = vec![2, 3, 5];
    let task = cpu.try_reserve()?.submit_with_context(move |context| {
        assert!(!context.is_cancelled());
        assert_ne!(context.identity().epoch(), 0);
        input
    });
    assert_eq!(task.join()?, [2, 3, 5]);
    Ok(())
}
