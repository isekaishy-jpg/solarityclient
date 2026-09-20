//! Controlled background demand, admission, and service-order contracts.

use solarity_cpu::{
    CpuError, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan, CpuTask, FrameBatch,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

/// Explicit one-lane fixture makes queue order independent of OS scheduling.
fn cpu(workers: usize, capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(workers).ok_or("zero workers")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(capacity).ok_or("zero capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

/// A confirmed-running operation holds dispatch while all successors are queued.
fn hold(cpu: &CpuExecutor) -> Result<(mpsc::Sender<()>, CpuTask<()>), Box<dyn Error>> {
    let (release, wait) = mpsc::channel();
    let (started, observed) = mpsc::channel();
    let task = cpu.try_submit(move || {
        let _sent = started.send(());
        let _released = wait.recv();
    })?;
    observed.recv_timeout(Duration::from_secs(5))?;
    Ok((release, task))
}

#[test]
fn promotion_withdrawal_and_retirement_turns_preserve_one_result_per_request()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(1, 16)?;
    let (release, blocked) = hold(&cpu)?;
    let (record, order) = mpsc::channel();
    let mut tasks = Vec::new();
    for (class, value) in [
        (CpuService::Speculative, 1),
        (CpuService::Speculative, 2),
        (CpuService::Required, 3),
        (CpuService::Retirement, 4),
        (CpuService::Required, 5),
        (CpuService::Retirement, 6),
    ] {
        let record = record.clone();
        tasks.push(cpu.try_submit_for(class, move || record.send(value))?);
    }
    tasks[1].set_service(CpuService::Required);
    tasks[2].set_service(CpuService::Speculative);
    // Repeating demand changes must neither duplicate work nor reorder FIFO ties.
    tasks[1].set_service(CpuService::Required);
    release.send(())?;
    // Observe before joining: a join is itself required demand.
    let actual = (0..6)
        .map(|_| order.recv_timeout(Duration::from_secs(5)))
        .collect::<Result<Vec<_>, _>>()?;
    blocked.join()?;
    for task in tasks {
        task.join()??;
    }
    cpu.shutdown()?;
    assert_eq!(actual, [4, 5, 6, 2, 1, 3]);
    assert!(order.try_recv().is_err());
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}

#[test]
fn speculative_reservations_leave_required_admission_and_return_unused_capacity()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu(2, 3)?;
    let first = cpu.try_reserve_for(CpuService::Speculative)?;
    let second = cpu.try_reserve_for(CpuService::Speculative)?;
    assert!(matches!(
        cpu.try_reserve_for(CpuService::Speculative),
        Err(CpuError::AtCapacity { .. })
    ));
    let required = cpu.try_reserve()?;
    assert_eq!(cpu.snapshot()?.in_flight(), 3);
    drop((first, second, required));
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    cpu.shutdown()?;
    assert!(matches!(
        cpu.try_reserve_for(CpuService::Speculative),
        Err(CpuError::ShuttingDown)
    ));
    Ok(())
}

#[test]
fn speculation_does_not_run_ahead_of_ready_frame_work() -> Result<(), Box<dyn Error>> {
    let cpu = cpu(1, 8)?;
    let (release, blocked) = hold(&cpu)?;
    let (record, order) = mpsc::channel();
    let report = record.clone();
    let speculative = cpu.try_submit_for(CpuService::Speculative, move || report.send(2))?;
    let mut batch = FrameBatch::new(|input: &mut mpsc::Sender<u8>| {
        let _sent = input.send(1);
    });
    let mut inputs = vec![record];
    batch.start(&cpu, &mut inputs)?;
    release.send(())?;
    let actual = [
        order.recv_timeout(Duration::from_secs(5))?,
        order.recv_timeout(Duration::from_secs(5))?,
    ];
    blocked.join()?;
    speculative.join()??;
    batch.reclaim(&mut inputs)?;
    assert_eq!(actual, [1, 2]);
    Ok(())
}
