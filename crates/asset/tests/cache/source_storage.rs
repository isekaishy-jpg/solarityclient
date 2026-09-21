//! Namespace startup admission is atomic and tracks actual control lifetimes.
use super::*;
use crate::{BlpCacheService, M2CacheService, WmoCacheService};
use solarity_cpu::CpuStoragePlan;
use std::error::Error;

#[test]
fn namespace_controls_bind_together_and_release_with_each_service() -> Result<(), Box<dyn Error>> {
    let storage = Arc::new(SourceStorage::default());
    let models = M2CacheService::with_storage(Arc::clone(&storage));
    let worlds = WmoCacheService::with_storage(Arc::clone(&storage));
    let textures = BlpCacheService::with_storage(Arc::clone(&storage));
    let own_bytes = size_of::<SourceStorage>() + 2 * size_of::<usize>();
    let controls: usize = storage
        .records
        .lock()
        .map_err(|_| "records")?
        .iter()
        .flatten()
        .map(|record| record.bytes)
        .sum();
    let required = own_bytes + controls;
    assert_eq!(
        storage
            .records
            .lock()
            .map_err(|_| "records")?
            .iter()
            .flatten()
            .count(),
        CONTROLS
    );
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(0, required - 1, 0));
    assert!(models.configure_storage(refused.clone()).is_err());
    assert_eq!(refused.snapshot().used(CpuStorageClass::Required), 0);
    assert!(storage.binding.get().is_none());
    assert!(
        storage
            .records
            .lock()
            .map_err(|_| "records")?
            .iter()
            .flatten()
            .all(|record| record.memory.is_none())
    );
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, required, 0));
    models.configure_storage(budget.clone())?;
    assert_eq!(
        budget
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Metadata),
        required
    );
    let copies = (models.clone(), worlds.clone(), textures.clone());
    drop((models, worlds, textures));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), required);
    drop(copies.0);
    let after_models = budget.snapshot().used(CpuStorageClass::Required);
    assert!(after_models < required && after_models > own_bytes);
    drop(copies.1);
    assert!(budget.snapshot().used(CpuStorageClass::Required) < after_models);
    drop(copies.2);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), own_bytes);
    drop(storage);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn retired_signal_stays_charged_until_the_last_observer_drops() -> Result<(), Box<dyn Error>> {
    let storage = Arc::new(SourceStorage::default());
    let signal = RetirementSignal::new(&storage);
    let observer = signal.observer();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    storage.configure(budget.clone())?;
    let charged = budget.snapshot().used(CpuStorageClass::Required);
    observer.notify();
    assert!(signal.swap(false, Ordering::AcqRel));
    drop((signal, storage));
    assert!(!observer.is_alive());
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), charged);
    drop(observer);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn concurrent_configuration_publishes_one_budget_without_leaking_the_other()
-> Result<(), Box<dyn Error>> {
    let service = M2CacheService::default();
    let first = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let second = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(|| service.configure_storage(first.clone()));
        let right = scope.spawn(|| service.configure_storage(second.clone()));
        (left.join(), right.join())
    });
    let left = left.map_err(|_| "configuration panic")?;
    let right = right.map_err(|_| "configuration panic")?;
    assert_ne!(left.is_ok(), right.is_ok());
    if left.is_ok() {
        assert_eq!(second.snapshot().used(CpuStorageClass::Required), 0);
    } else {
        assert_eq!(first.snapshot().used(CpuStorageClass::Required), 0);
    }
    drop(service);
    assert_eq!(first.snapshot().used(CpuStorageClass::Required), 0);
    assert_eq!(second.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn cache_notification_prunes_retired_signals_without_allocating() -> Result<(), Box<dyn Error>> {
    let storage = Arc::new(SourceStorage::default());
    let signal = RetirementSignal::new(&storage);
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    storage.configure(budget.clone())?;
    let mut cache = crate::cache::resource::ResourceCache::<u32, u32>::default();
    cache.admit(Some(&budget))?;
    cache.subscribe(&signal)?;
    let before = budget.snapshot().used(CpuStorageClass::Required);
    drop(signal);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), before);
    let ((), calls) = crate::test_allocations::count(|| cache.mark_changed());
    assert_eq!(calls, 0);
    assert!(budget.snapshot().used(CpuStorageClass::Required) < before);
    drop((cache, storage));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
