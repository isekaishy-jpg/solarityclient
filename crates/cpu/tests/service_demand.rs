//! Registered consumers change producer priority without sharing its result handle.

use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuService, CpuServiceDemand, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize, sync::mpsc};

/// A held worker makes every observed transition apply to an actually queued task.
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
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

/// Parent controls obtained before and after dispatch must see the same scoped demand.
#[test]
fn nested_source_cannot_demote_parent_and_expired_controls_cannot_promote_it()
-> Result<(), Box<dyn Error>> {
    let mut cpu = pool()?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let permit = cpu.try_reserve()?;
    let parent = permit.service_control();
    let scope = parent.scoped_demand(CpuService::Speculative)?;
    let source = CpuServiceDemand::default();
    let interest = source.subscribe(CpuService::Speculative);
    assert!(source.bind(scope.control()));
    let task = permit.submit(|| 42);
    let later = task.service_control();
    let mut observed = vec![parent.service()];
    task.set_service(CpuService::Speculative);
    observed.push(parent.service());
    interest.set_service(CpuService::Required);
    observed.push(later.service());
    later.set_service(CpuService::Retirement);
    observed.push(parent.service());
    drop(scope);
    observed.push(later.service());
    interest.set_service(CpuService::Speculative);
    interest.set_service(CpuService::Required);
    observed.push(parent.service());
    release.send(())?;
    blocker.join()??;
    assert_eq!(task.join()?, 42);
    assert_eq!(
        observed,
        [
            CpuService::Required,
            CpuService::Speculative,
            CpuService::Required,
            CpuService::Required,
            CpuService::Retirement,
            CpuService::Retirement,
        ]
    );
    cpu.shutdown()?;
    Ok(())
}

/// Completing one source must not clear another active source's promotion.
#[test]
fn sibling_producers_withdraw_independently_and_restore_current_owner_demand()
-> Result<(), Box<dyn Error>> {
    let mut cpu = pool()?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let task = cpu.try_submit_for(CpuService::Speculative, || 42)?;
    let owner = task.service_control();
    let first = owner.scoped_demand(CpuService::Required)?;
    let second = owner.scoped_demand(CpuService::Retirement)?;
    let control = second.control();
    let mut observed = vec![owner.service()];
    drop(first);
    observed.push(owner.service());
    owner.set_service(CpuService::Required);
    drop(second);
    observed.push(owner.service());
    owner.set_service(CpuService::Speculative);
    control.set_service(CpuService::Required);
    observed.push(owner.service());
    release.send(())?;
    blocker.join()??;
    assert_eq!(task.join()?, 42);
    assert_eq!(
        observed,
        [
            CpuService::Required,
            CpuService::Retirement,
            CpuService::Required,
            CpuService::Speculative
        ]
    );
    cpu.shutdown()?;
    Ok(())
}

/// Budget refusal is atomic and retained stale controls keep their metadata charged.
#[test]
fn scoped_demand_admits_metadata_before_publication_and_releases_last_control()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuError, CpuStorageClass as Class, CpuStorageKind as Kind};
    let mut cpu = pool()?;
    let permit = cpu.try_reserve()?;
    let owner = permit.service_control();
    let baseline = cpu.storage().snapshot().used(Class::Required);
    let snapshot = cpu.storage().snapshot();
    let pressure = cpu.storage().reserve(
        Class::Required,
        Kind::Scratch,
        snapshot.limit(Class::Required) - snapshot.used(Class::Required),
    )?;
    assert!(matches!(
        owner.scoped_demand(CpuService::Speculative),
        Err(CpuError::StorageAtCapacity { .. })
    ));
    assert_eq!(owner.service(), CpuService::Required);
    drop(pressure);
    assert_eq!(cpu.storage().snapshot().used(Class::Required), baseline);
    let scope = owner.scoped_demand(CpuService::Required)?;
    let control = scope.control();
    let charged = cpu.storage().snapshot().used(Class::Required);
    assert!(charged > baseline);
    drop(scope);
    assert_eq!(cpu.storage().snapshot().used(Class::Required), charged);
    drop(control);
    assert_eq!(cpu.storage().snapshot().used(Class::Required), baseline);
    drop((permit, owner));
    cpu.shutdown()?;
    Ok(())
}
