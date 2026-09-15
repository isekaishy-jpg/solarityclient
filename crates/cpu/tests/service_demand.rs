//! Registered consumers change producer priority without sharing its result handle.

use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuService, CpuServiceDemand, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize, sync::mpsc};

/// A held worker makes every observed transition apply to an actually queued task.
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        // Leave the required admission slot free while a blocker and prewarm coexist.
        NonZeroUsize::new(3).ok_or("positive capacity required")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// Clones share one consumer; removing a required owner restores remaining speculative demand.
#[test]
fn strongest_live_consumer_controls_queue_class_through_release_and_clone()
-> Result<(), Box<dyn Error>> {
    let mut cpu = pool()?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let task = cpu.try_submit_for(CpuService::Speculative, || 42)?;
    let control = task.service_control();
    let demand = CpuServiceDemand::default();
    let prewarm = demand.subscribe(CpuService::Speculative);
    let bound = demand.bind(control.clone());
    let mut observed = vec![control.service()];
    let selected = demand.subscribe(CpuService::Required);
    observed.push(control.service());
    let clone = selected.clone();
    drop(selected);
    observed.push(control.service());
    drop(clone);
    observed.push(control.service());
    prewarm.set_service(CpuService::Retirement);
    observed.push(control.service());
    drop(prewarm);
    observed.push(control.service());
    let duplicate = demand.bind(control);
    // Always unblock before assertions so a regression cannot strand the pool during unwind.
    release.send(())?;
    blocker.join()??;
    assert_eq!(task.join()?, 42);
    assert!(bound);
    assert!(!duplicate);
    assert_eq!(
        observed,
        [
            CpuService::Speculative,
            CpuService::Required,
            CpuService::Required,
            CpuService::Speculative,
            CpuService::Retirement,
            CpuService::Retirement
        ]
    );
    cpu.shutdown()?;
    Ok(())
}

/// Late binding sees current demand, including withdrawal before a producer is dispatched.
#[test]
fn demand_registered_before_binding_applies_only_current_consumers() -> Result<(), Box<dyn Error>> {
    let mut cpu = pool()?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let task = cpu.try_submit_for(CpuService::Required, || 7)?;
    let control = task.service_control();
    let demand = CpuServiceDemand::default();
    let selected = demand.subscribe(CpuService::Required);
    let prewarm = demand.subscribe(CpuService::Speculative);
    drop(selected);
    let bound = demand.bind(control.clone());
    let observed = control.service();
    release.send(())?;
    blocker.join()??;
    assert_eq!(task.join()?, 7);
    assert!(bound);
    assert_eq!(observed, CpuService::Speculative);
    drop(prewarm);
    cpu.shutdown()?;
    Ok(())
}
