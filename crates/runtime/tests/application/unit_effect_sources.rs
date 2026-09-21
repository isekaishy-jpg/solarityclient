//! One flexible worker can service other jobs while an effect joins a shared model.

use super::source_steps;
use crate::application::RuntimeTerrainError;
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_frame::m2::unit_effects::PreparedUnitEffects;
use crate::test_support::unit_models;
use solarity_asset::{ArchiveCatalog, EnvironmentalDamageCatalog};
use solarity_asset::{
    AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, M2Load, ResourceLease,
};
use solarity_cpu::{CpuError, CpuTaskStep};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStorageKind, CpuStoragePlan,
};
use std::sync::Arc;
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

fn executor() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?)
}

#[test]
fn shared_effect_source_suspends_and_retains_admitted_completion_records()
-> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture_with_water_effects(0)?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let environmental = Arc::new(EnvironmentalDamageCatalog::load(&mut store)?);
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("World/WaterEffect.mdx")?,
    );
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("expected source producer".into());
    };
    let cpu = executor()?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let (sent, suspended) = mpsc::channel();
    let mut operation = source_steps(catalog.clone(), environmental, shared);
    let task = permit.submit_resumable_with_context(move |context| {
        let result = operation(context);
        if matches!(result, CpuTaskStep::Wait(_)) {
            let _ = sent.send(());
        }
        result
    });
    suspended.recv_timeout(Duration::from_secs(5))?;
    assert!(!task.is_finished());
    assert_eq!(cpu.try_reserve()?.submit(|| 42).join()?, 42);
    let expected = producer.load(&mut store)?;
    let effects = task.join()??;
    assert_eq!(effects.len(), 5);
    assert!(effects.iter().all(Option::is_some));
    let M2Load::Ready(retained) = catalog.model_cache_service().request(&key)? else {
        return Err("source was not retained".into());
    };
    assert!(ResourceLease::ptr_eq(&expected, &retained));
    assert!(
        cpu.storage()
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Result)
            > 0
    );
    drop(effects);
    assert_eq!(
        cpu.storage()
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Result),
        0
    );
    Ok(())
}

#[test]
fn withdrawn_effect_consumer_releases_its_dependency_without_abandoning_producer()
-> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture_with_water_effects(0)?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let environmental = Arc::new(EnvironmentalDamageCatalog::load(&mut store)?);
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("World/WaterEffect.m2")?);
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("expected source producer".into());
    };
    let observer = producer.subscribe()?;
    let cpu = executor()?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let (sent, suspended) = mpsc::channel();
    let mut operation = source_steps(catalog, environmental, shared);
    let task = permit.submit_resumable_with_context(move |context| {
        let result = operation(context);
        if matches!(result, CpuTaskStep::Wait(_)) {
            let _ = sent.send(());
        }
        result
    });
    suspended.recv_timeout(Duration::from_secs(5))?;
    task.cancel();
    assert!(matches!(
        task.join()?,
        Err(RuntimeTerrainError::Cpu(CpuError::JobCancelled))
    ));
    assert_eq!(
        cpu.storage()
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Result),
        0
    );
    assert!(observer.poll().is_none());
    let expected = producer.load(&mut store)?;
    assert!(ResourceLease::ptr_eq(
        &expected,
        &observer.poll().ok_or("producer was abandoned")??
    ));
    Ok(())
}

/// Scene parity fixtures now enter sources through the production CPU continuation.
pub(in crate::application) fn prepare_sources_for_test(
    catalog: ArchiveCatalog,
    environmental: Arc<EnvironmentalDamageCatalog>,
) -> Result<PreparedUnitEffects, Box<dyn Error>> {
    let cpu = executor()?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    Ok(permit
        .submit_resumable_with_context(source_steps(catalog, environmental, shared))
        .join()??)
}
