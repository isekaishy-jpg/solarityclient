//! Owner-list pressure and incremental retirement without an allocating cleanup snapshot.
use super::*;
use crate::{M2ModelCache, test_allocations};
use solarity_cpu::{CpuStorageBudget, CpuStoragePlan};
use std::error::Error;

fn service() -> Result<(M2CacheService, CpuStorageBudget), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let service = M2CacheService::default();
    service.configure_storage(budget.clone())?;
    Ok((service, budget))
}
fn registered(service: &M2CacheService) -> Result<M2ModelCache, Box<dyn Error>> {
    let cache = M2ModelCache::new();
    service.register(&cache.core)?;
    Ok(cache)
}

#[test]
fn full_budget_retirement_needs_no_owner_snapshot_or_allocation() -> Result<(), Box<dyn Error>> {
    let (service, budget) = service()?;
    let first = registered(&service)?;
    let second = registered(&service)?;
    let third = registered(&service)?;
    drop((first, second, third));
    assert_eq!(service.next_collection_delay_ms(), Some(0));
    let before = budget.snapshot().used(CpuStorageClass::Required);
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - before,
    )?;
    let (turns, calls) = test_allocations::count(|| {
        let mut collection = service.begin_collection();
        let mut turns = 1;
        while collection.step().is_continue() {
            turns += 1;
        }
        turns
    });
    assert_eq!(turns, 3, "one closed owner per finite step");
    assert_eq!(calls, 0);
    assert_eq!(service.next_collection_delay_ms(), None);
    assert!(budget.snapshot().used(CpuStorageClass::Required) < 65536);
    drop(hold);
    assert!(
        budget.snapshot().used(CpuStorageClass::Required) > 0,
        "reusable registry capacity stays charged"
    );
    drop(service);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn overlapping_collection_and_slot_reuse_preserve_later_owner_demand() -> Result<(), Box<dyn Error>>
{
    let (service, budget) = service()?;
    let active = registered(&service)?;
    let closed = registered(&service)?;
    let mut first = service.begin_collection();
    let mut overlap = service.begin_collection();
    assert!(first.step().is_continue());
    drop(closed);
    assert!(first.step().is_break());
    let reused = registered(&service)?;
    let appended = registered(&service)?;
    assert_eq!(
        service
            .0
            .owners
            .lock()
            .map_err(|_| "registry lock")?
            .entries
            .len(),
        3
    );
    drop((reused, appended));
    assert!(overlap.step().is_continue());
    assert!(overlap.step().is_break());
    assert!(
        first.step().is_break(),
        "finished passes do not consume later registrations"
    );
    assert_eq!(service.0.owners.lock().map_err(|_| "registry lock")?.len, 2);
    assert!(service.take_changed());
    assert_eq!(
        service.next_collection_delay_ms(),
        Some(0),
        "appended owner remains durably pending"
    );
    let mut next = service.begin_collection();
    while next.step().is_continue() {}
    assert_eq!(service.next_collection_delay_ms(), None);
    assert_eq!(service.0.owners.lock().map_err(|_| "registry lock")?.len, 1);
    // Abandoning a partially visited pass leaves source ownership with the registry.
    let mut abandoned = service.begin_collection();
    assert!(abandoned.step().is_continue());
    drop(abandoned);
    drop(active);
    assert_eq!(service.next_collection_delay_ms(), Some(0));
    let mut last = service.begin_collection();
    while last.step().is_continue() {}
    drop((first, overlap, next, last, service));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn registration_pressure_does_not_publish_an_owner_or_maintenance_observer()
-> Result<(), Box<dyn Error>> {
    let (service, budget) = service()?;
    let cache = M2ModelCache::new();
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - budget.snapshot().used(CpuStorageClass::Required),
    )?;
    assert!(service.register(&cache.core).is_err());
    assert!(!service.take_changed());
    assert_eq!(service.next_collection_delay_ms(), None);
    drop(hold);
    service.register(&cache.core)?;
    assert!(service.take_changed());
    let another = M2ModelCache::new();
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - budget.snapshot().used(CpuStorageClass::Required),
    )?;
    assert!(service.register(&another.core).is_err());
    assert!(!service.take_changed());
    assert_eq!(service.0.owners.lock().map_err(|_| "registry lock")?.len, 1);
    drop(hold);
    service.register(&another.core)?;
    assert_eq!(service.0.owners.lock().map_err(|_| "registry lock")?.len, 2);
    drop((cache, another));
    let mut collection = service.begin_collection();
    while collection.step().is_continue() {}
    drop((collection, service));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
