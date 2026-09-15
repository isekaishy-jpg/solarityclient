//! Real decoded models retire through the CPU service even without subsequent loads.

use super::RuntimeModelCacheMaintenance;
use crate::test_support::{ClientFixture, game_object_models};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, M2ModelCache,
    ResourceCacheClock, ResourceLease,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

/// One admitted task makes capacity refusal deterministic without a blocking worker.
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// Portable source/skin bytes exercise the production archive and cache registration path.
fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    ClientFixture::with_common_files(&[
        (
            "World/Retired.m2",
            &game_object_models::model_with_animations(&[0])?,
        ),
        ("World/Retired00.skin", &game_object_models::skin()?),
    ])
}

/// A deadline with no new loads survives pool pressure and then retires through admitted CPU work.
#[test]
fn idle_model_retirement_preserves_sources_under_saturation() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let mut maintenance = RuntimeModelCacheMaintenance::new(sources.clone());
    let mut store = AssetStore::mount(catalog)?;
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        M2ModelCache::with_clock(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));
    let model = cache.load(&mut store, &AssetPath::new("World/Retired.m2")?)?;
    let weak = ResourceLease::downgrade(&model);
    drop(model);
    let mut cpu = pool()?;
    let now = Instant::now();
    maintenance.service_at(&cpu, now)?;
    assert!(maintenance.pending.is_none());
    assert_eq!(maintenance.deadline, Some(now + Duration::from_secs(10)));
    clock.store(9_999, Ordering::Release);
    maintenance.service_at(&cpu, now + Duration::from_millis(9_999))?;
    assert!(maintenance.pending.is_none());
    assert!(weak.is_alive());
    let occupied = cpu.try_reserve()?;
    clock.store(10_000, Ordering::Release);
    maintenance.service_at(&cpu, now + Duration::from_secs(10))?;
    assert!(maintenance.pending.is_none());
    assert!(weak.is_alive());
    drop(occupied);
    maintenance.service_at(&cpu, now + Duration::from_secs(10))?;
    maintenance
        .pending
        .take()
        .ok_or("cleanup was not admitted")?
        .join()?;
    assert!(!weak.is_alive());
    assert!(cache.is_empty());
    assert_eq!(sources.next_collection_delay_ms(), None);
    maintenance.service_at(&cpu, now + Duration::from_secs(11))?;
    assert!(
        maintenance.pending.is_none(),
        "empty cache must not keep scheduling cleanup"
    );
    cpu.shutdown()?;
    Ok(())
}

/// Closing a worker cache transfers its retained ownership into the namespace retirement service.
#[test]
fn closed_cache_waits_for_worker_maintenance_and_preserves_live_consumers()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut maintenance = RuntimeModelCacheMaintenance::new(catalog.model_cache_service());
    let clone_service = catalog.clone().model_cache_service();
    let independent =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?
            .model_cache_service();
    let mut store = AssetStore::mount(catalog)?;
    let mut cache = M2ModelCache::new();
    let model = cache.load(&mut store, &AssetPath::new("World/Retired.m2")?)?;
    let weak = ResourceLease::downgrade(&model);
    drop(cache);
    assert_eq!(clone_service.next_collection_delay_ms(), Some(0));
    assert_eq!(independent.next_collection_delay_ms(), None);
    assert!(weak.is_alive());
    let mut cpu = pool()?;
    maintenance.service(&cpu)?;
    maintenance
        .pending
        .take()
        .ok_or("closed cache cleanup was not admitted")?
        .join()?;
    assert!(
        weak.is_alive(),
        "live model survives removal of cache ownership"
    );
    assert_eq!(clone_service.next_collection_delay_ms(), None);
    drop(model);
    assert!(!weak.is_alive());
    cpu.shutdown()?;
    Ok(())
}
