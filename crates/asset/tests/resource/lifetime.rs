//! Resource lifetime tests use owned payloads and controlled consumer boundaries.

#[path = "allocations.rs"]
mod allocations;

use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use super::super::{ResourceLease, releases::Releases};
use super::ResourceCache;

/// Cache ownership survives zero consumers; reacquisition preserves exact payload identity.
#[test]
fn released_resource_can_be_reacquired_before_collection() -> Result<(), Box<dyn Error>> {
    let mut cache = ResourceCache::default();
    let first = cache.insert(1u32, String::from("model"))?;
    let observer = ResourceLease::downgrade(&first);
    let identity = ResourceLease::as_ptr(&first);
    let second = cache.get(&1).ok_or("missing live resource")?;
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
    let reacquired = cache.get(&1).ok_or("missing retained resource")?;
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
    assert_eq!(releases.pop(), Some(first));
    assert_eq!(releases.pop(), None);
    releases.unregister(first);
    let second = releases.register()?;
    assert_eq!(first.index, second.index);
    assert_ne!(first, second);
    releases.notify(first);
    assert_eq!(releases.pop(), None);
    releases.notify(second);
    releases.unregister(second);
    assert_eq!(releases.pop(), None);
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
    let first = cache.insert(7u32, 42u64)?;
    let (result, calls) = allocations::count(|| -> Result<usize, &'static str> {
        for _ in 0..1000 {
            let clone = first.clone();
            let hit = cache.get(&7).ok_or("live entry missing")?;
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
        assert_eq!(self.1.pop(), None);
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
