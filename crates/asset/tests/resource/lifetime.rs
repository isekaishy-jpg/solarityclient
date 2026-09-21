//! Resource lifetime tests use owned payloads and controlled consumer boundaries.

use crate::test_allocations as allocations;

use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
};

use super::super::{ResourceCacheClock, ResourceLease, releases::Releases};
use super::ResourceCache;
use solarity_cpu::{
    CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan,
};

/// Cache ownership survives zero consumers; reacquisition preserves exact payload identity.
#[test]
fn released_resource_can_be_reacquired_before_collection() -> Result<(), Box<dyn Error>> {
    let mut cache = ResourceCache::default();
    let first = cache.insert(1u32, String::from("model"))?;
    let observer = ResourceLease::downgrade(&first);
    let identity = ResourceLease::as_ptr(&first);
    let second = cache.get(&1)?.ok_or("missing live resource")?;
    assert!(ResourceLease::ptr_eq(&first, &second));
    drop(first);
    assert_eq!(cache.collect_unused(), 0);
    drop(second);
    assert!(
        observer.is_alive(),
        "the cache still owns the immutable generation"
    );
    assert!(
        observer.upgrade().is_none(),
        "weak observation cannot create new cache demand"
    );
    let reacquired = cache.get(&1)?.ok_or("missing retained resource")?;
    assert_eq!(ResourceLease::as_ptr(&reacquired), identity);
    assert_eq!(
        cache.collect_unused(),
        0,
        "old release cannot evict reacquired data"
    );
    drop(reacquired);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    assert!(!observer.is_alive());
    Ok(())
}

/// Thousands of release/reacquire cycles retain one bounded notification slot.
#[test]
fn release_churn_coalesces_and_recycled_slots_reject_old_generations() -> Result<(), Box<dyn Error>>
{
    let releases = Releases::default();
    let first = releases.register()?;
    for _ in 0..1000 {
        releases.notify(first);
    }
    assert_eq!(releases.pop(0), Some(first));
    assert_eq!(releases.pop(0), None);
    releases.unregister(first);
    let second = releases.register()?;
    assert_eq!(first.index, second.index);
    assert_ne!(first, second);
    releases.notify(first);
    assert_eq!(releases.pop(0), None);
    releases.notify(second);
    releases.unregister(second);
    assert_eq!(releases.pop(0), None);
    Ok(())
}

/// Concurrent final clones release one pin; cache disposal cannot invalidate another consumer.
#[test]
fn worker_release_and_cache_teardown_preserve_live_payloads() -> Result<(), Box<dyn Error>> {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut cache = ResourceCache::default();
    let resource = cache.insert(1u32, DropCount(Arc::clone(&drops)))?;
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let clone = resource.clone();
            scope.spawn(move || drop(clone));
        }
    });
    assert_eq!(cache.collect_unused(), 0);
    drop(cache);
    assert_eq!(drops.load(Ordering::Acquire), 0);
    drop(resource);
    assert_eq!(drops.load(Ordering::Acquire), 1);
    Ok(())
}

/// Only released entries retire, and a source destructor runs outside release metadata.
#[test]
fn collection_drops_sources_without_holding_the_release_queue() -> Result<(), Box<dyn Error>> {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut cache = ResourceCache::default();
    let queue = Arc::clone(&cache.releases);
    let first = cache.insert(1u32, DropProbe(Arc::clone(&drops), Arc::clone(&queue)))?;
    let second = cache.insert(2u32, DropProbe(Arc::clone(&drops), queue))?;
    drop(first);
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert_eq!(cache.len(), 1);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(drops.load(Ordering::Acquire), 2);
    Ok(())
}

/// Hot clones/hits and final-release delivery require no new task or message allocation.
#[test]
fn warm_consumers_and_final_release_allocate_nothing() -> Result<(), Box<dyn Error>> {
    let mut cache = ResourceCache::default();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    cache.admit(Some(&budget))?;
    let first = cache.insert(7u32, 42u64)?;
    let (result, calls) = allocations::count(|| -> Result<usize, &'static str> {
        for _ in 0..1000 {
            let clone = first.clone();
            let hit = cache
                .get(&7)
                .map_err(|_| "generation exhausted")?
                .ok_or("live entry missing")?;
            assert!(ResourceLease::ptr_eq(&clone, &hit));
            drop((clone, hit));
        }
        drop(first);
        Ok(cache.collect_unused())
    });
    assert_eq!(result?, 1);
    assert_eq!(calls, 0);
    Ok(())
}

/// Final release on a worker delivers the same allocation-free registered notification.
#[test]
fn final_worker_release_is_delivered_without_allocating() -> Result<(), Box<dyn Error>> {
    let mut cache = ResourceCache::default();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    cache.admit(Some(&budget))?;
    let resource = cache.insert(7u32, 42u64)?;
    let calls = std::thread::spawn(move || allocations::count(|| drop(resource)).1)
        .join()
        .map_err(|_| "release worker panicked")?;
    assert_eq!(calls, 0);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    Ok(())
}

/// Source disposal reenters the same metadata queue; holding that lock would deadlock.
struct DropProbe(Arc<AtomicUsize>, Arc<Releases>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        assert_eq!(self.1.pop(0), None);
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

/// Explicit destruction observation avoids relying on Arc implementation counts.
struct DropCount(Arc<AtomicUsize>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

/// Native 81C290 uses wrapping subtraction followed by signed JL at 10,000 ms.
#[test]
fn qualified_collection_matches_stock_age_wrap_and_signed_boundary() -> Result<(), Box<dyn Error>> {
    for released in [0u32, u32::MAX - 5_000] {
        let clock = Arc::new(AtomicU32::new(released));
        let mut cache = ResourceCache::with_retention(ResourceCacheClock::from_milliseconds(
            Arc::clone(&clock),
        ));
        let value = cache.insert(1u32, 42u32)?;
        let weak = ResourceLease::downgrade(&value);
        drop(value);
        assert_eq!(cache.next_delay_ms(), Some(10_000));
        clock.store(released.wrapping_add(9_999), Ordering::Release);
        assert_eq!(cache.collect_unused(), 0);
        assert!(weak.is_alive());
        clock.store(released.wrapping_add(0x8000_0000), Ordering::Release);
        assert_eq!(
            cache.collect_unused(),
            0,
            "negative signed age is not expired"
        );
        clock.store(released.wrapping_add(10_000), Ordering::Release);
        assert_eq!(cache.collect_unused(), 1);
        assert!(!weak.is_alive());
    }
    Ok(())
}

/// A delayed destructor from the old pin cannot start the reacquired pin's grace period.
#[test]
fn reacquisition_rejects_old_release_even_before_new_pin_releases() -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        ResourceCache::with_retention(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));
    let first = cache.insert(1u32, 42u32)?;
    let old_ticket = cache.entries[0].as_ref().ok_or("missing entry")?.ticket;
    drop(first);
    clock.store(5_000, Ordering::Release);
    let active = cache.get(&1)?.ok_or("missing generation")?;
    cache.releases.notify(old_ticket);
    assert_eq!(cache.next_delay_ms(), None);
    clock.store(9_000, Ordering::Release);
    drop(active);
    cache.releases.notify(old_ticket);
    clock.store(18_999, Ordering::Release);
    assert_eq!(cache.collect_unused(), 0);
    clock.store(19_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    Ok(())
}

/// Reacquiring a middle entry preserves the remaining release order and its own new deadline.
#[test]
fn qualified_release_queue_keeps_oldest_due_prefix_after_middle_reacquisition()
-> Result<(), Box<dyn Error>> {
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        ResourceCache::with_retention(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));
    for key in 0..3u32 {
        clock.store(key * 1_000, Ordering::Release);
        drop(cache.insert(key, key)?);
    }
    let middle = cache.get(&1)?.ok_or("missing middle")?;
    clock.store(3_000, Ordering::Release);
    drop(middle);
    clock.store(11_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(cache.next_delay_ms(), Some(1_000));
    clock.store(12_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(cache.next_delay_ms(), Some(1_000));
    clock.store(13_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    Ok(())
}

/// Adding the clock and durable observer does not allocate on a final consumer release.
#[test]
fn qualified_release_notifies_idle_maintenance_without_allocation() -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        ResourceCache::with_retention(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));
    let changed = Arc::new(AtomicBool::new(false));
    cache.subscribe(&changed)?;
    let value = cache.insert(1u32, 42u32)?;
    changed.store(false, Ordering::Release);
    let (_, calls) = allocations::count(|| drop(value));
    assert_eq!(calls, 0);
    assert!(changed.load(Ordering::Acquire));
    clock.store(10_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    Ok(())
}

/// Refused reacquisition must not withdraw the old deadline or change source identity.
#[test]
fn admitted_cache_pressure_preserves_live_hits_and_expired_release() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        ResourceCache::with_retention(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));
    cache.admit(Some(&budget))?;
    let lease = cache.insert(1u32, 73u32)?;
    let weak = ResourceLease::downgrade(&lease);
    let held = budget.reserve(
        Class::Required,
        Kind::Scratch,
        65536 - budget.snapshot().used(Class::Required),
    )?;
    let hit = cache.get(&1)?.ok_or("live source missing")?;
    assert!(ResourceLease::ptr_eq(&lease, &hit));
    assert!(cache.insert(2, 91).is_err());
    assert_eq!(cache.len(), 1);
    drop((lease, hit));
    clock.store(5000, Ordering::Release);
    assert!(cache.get(&1).is_err());
    assert_eq!(cache.next_delay_ms(), Some(5000));
    assert!(weak.upgrade().is_none());
    clock.store(10000, Ordering::Release);
    let (count, calls) = allocations::count(|| cache.collect_unused());
    assert_eq!(count, 1);
    assert_eq!(calls, 0);
    assert!(!weak.is_alive());
    drop(held);
    let replacement = cache.insert(2, 91)?;
    assert_eq!(*replacement, 91);
    drop((cache, replacement, weak));
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}

/// Weak observers charge the pin allocation after the consumer and cache have retired.
#[test]
fn cached_pin_metadata_survives_cache_then_strong_then_weak_retirement()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let mut cache = ResourceCache::default();
    cache.admit(Some(&budget))?;
    let lease = cache.insert(1u32, 73u32)?;
    let weak = ResourceLease::downgrade(&lease);
    drop(cache);
    let live_bytes = budget.snapshot().used(Class::Required);
    assert!(live_bytes > 0);
    assert_eq!(*weak.upgrade().ok_or("lost consumer")?, 73);
    drop(lease);
    let weak_bytes = budget.snapshot().used(Class::Required);
    assert!(weak_bytes > 0 && weak_bytes < live_bytes);
    assert!(weak.upgrade().is_none());
    assert!(!weak.is_alive());
    let clone = weak.clone();
    drop(weak);
    assert_eq!(budget.snapshot().used(Class::Required), weak_bytes);
    drop(clone);
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}

/// Failed cache admission creates no lease, and failed slot growth leaves reusable tickets intact.
#[test]
fn cache_and_release_registration_refusal_are_retryable() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let mut cache = ResourceCache::<u32, u32>::default();
    let hold = budget.reserve(Class::Required, Kind::Scratch, 65536)?;
    assert!(cache.admit(Some(&budget)).is_err());
    assert!(cache.is_empty());
    drop(hold);
    cache.admit(Some(&budget))?;
    let releases = Releases::default();
    releases.admit(&crate::AssetReadBudget::for_service(
        budget.clone(),
        solarity_cpu::CpuService::Required,
    ))?;
    let first = releases.register()?;
    let hold = budget.reserve(
        Class::Required,
        Kind::Scratch,
        65536 - budget.snapshot().used(Class::Required),
    )?;
    assert!(releases.register().is_err());
    let watcher = Arc::new(AtomicBool::new(false));
    assert!(releases.subscribe(&watcher).is_err());
    assert!(!watcher.load(Ordering::Acquire));
    releases.notify(first);
    assert_eq!(releases.pop(0), Some(first));
    releases.unregister(first);
    let (replacement, calls) = allocations::count(|| releases.register());
    let replacement = replacement?;
    assert_eq!(calls, 0);
    assert_eq!(replacement.index, first.index);
    assert_ne!(replacement, first);
    releases.notify(first);
    assert_eq!(releases.pop(0), None);
    drop(hold);
    releases.subscribe(&watcher)?;
    assert!(watcher.load(Ordering::Acquire));
    drop((releases, cache));
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}
