//! Shared WMO roots preserve strict group decoding, namespace identity and retirement.
use super::{group_fixture, root_fixture};
use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, ResourceLease,
    WmoLoad, WmoLoadError,
};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass, CpuStoragePlan,
    CpuTaskStep, JobOutcome,
};
use std::{error::Error, num::NonZeroUsize, sync::Arc};

/// A strict root and one independently resolved group exercise ordinary archive precedence.
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World/Shared.wmo",
            bytes: &root_fixture(1),
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World/Shared_000.wmo",
            bytes: &group_fixture(8),
        },
    ])
}
/// A single worker makes any accidental consumer wait observable as lost progress.
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// A suspended root consumer releases the only worker and receives the same root/group lease.
#[test]
fn shared_world_model_dependency_releases_worker_and_retires_exact_generation()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let service = catalog.world_model_cache_service();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("World/Shared.wmo")?);
    let WmoLoad::Producer(producer) = service.request_for(&key, CpuService::Speculative)? else {
        return Err("missing producer".into());
    };
    let WmoLoad::Pending(request) = service.request_for(&key, CpuService::Required)? else {
        return Err("missing join".into());
    };
    let mut cpu = pool()?;
    let dependency = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    let mut edge = Some(dependency.task_dependency()?);
    let consumer = cpu.try_reserve()?.submit_resumable_with_context(move |_| {
        if let Some(edge) = edge.take() {
            return CpuTaskStep::Wait(edge);
        }
        CpuTaskStep::Complete(dependency.poll())
    });
    assert_eq!(cpu.try_submit(|| 41)?.join()?, 41);
    let mut reader = AssetStore::mount(catalog.clone())?;
    let root = cpu
        .try_submit(move || producer.load(&mut reader))?
        .join()??;
    let result = consumer.join()?.ok_or("missing durable source result")??;
    assert!(ResourceLease::ptr_eq(&root, &result));
    let late = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    assert_eq!(late.readiness().outcome()?, Some(JobOutcome::Succeeded));
    let WmoLoad::Ready(ready) = catalog
        .world_model_cache_service()
        .request_for(&key, CpuService::Required)?
    else {
        return Err("missing ready source".into());
    };
    assert!(ResourceLease::ptr_eq(&ready, &root));
    let other = ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let other_key = AssetResourceKey::new(other.namespace(), key.path().clone());
    assert!(matches!(
        other
            .world_model_cache_service()
            .request_for(&other_key, CpuService::Required)?,
        WmoLoad::Producer(_)
    ));
    let weak = ResourceLease::downgrade(&root);
    assert!(service.collect_step().is_break());
    assert!(weak.is_alive());
    drop((root, result, ready, late, request));
    assert!(service.take_changed());
    while service.collect_step().is_continue() {}
    assert!(!weak.is_alive());
    cpu.shutdown()?;
    Ok(())
}

/// Every joined error retains source identity; abandoned roots wake consumers and can be requested again.
#[test]
fn shared_world_model_failure_abandonment_and_namespace_are_terminal() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let service = catalog.world_model_cache_service();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("World/Missing.wmo")?);
    let WmoLoad::Producer(producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe_for(CpuService::Required);
    let mut cpu = pool()?;
    let edge = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    let Err(WmoLoadError::Asset(original)) =
        producer.load(&mut AssetStore::mount(catalog.clone())?)
    else {
        return Err("missing source error".into());
    };
    let Some(Err(WmoLoadError::Asset(delivered))) = edge.poll() else {
        return Err("missing dependency error".into());
    };
    assert!(Arc::ptr_eq(&original, &delivered));
    let WmoLoad::Producer(producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("failure was incorrectly cached".into());
    };
    let request = producer.subscribe_for(CpuService::Required);
    let edge = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    drop(producer);
    assert!(matches!(edge.poll(), Some(Err(WmoLoadError::Abandoned))));
    let WmoLoad::Producer(producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("abandoned producer retained authority".into());
    };
    let other = ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    assert!(matches!(
        producer.load(&mut AssetStore::mount(other)?),
        Err(WmoLoadError::Namespace { .. })
    ));
    cpu.shutdown()?;
    Ok(())
}

/// Root and group stages stay private; withdrawal never exposes a partially loaded WMO.
#[test]
fn world_model_steps_publish_only_after_group_validation() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let service = catalog.world_model_cache_service();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("World/Shared.wmo")?);
    let WmoLoad::Producer(mut producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("producer".into());
    };
    let request = producer.subscribe_for(CpuService::Required);
    let mut reader = AssetStore::mount(catalog.clone())?;
    assert!(producer.step(&mut reader)?.is_none()); // Root only.
    assert!(request.poll().is_none());
    assert!(producer.step(&mut reader)?.is_none()); // Group only.
    assert!(request.poll().is_none());
    let model = producer.step(&mut reader)?.ok_or("complete WMO")?;
    assert_eq!(model.groups().len(), 1);
    assert!(ResourceLease::ptr_eq(
        &model,
        &request.poll().ok_or("published WMO")??
    ));
    drop((producer, request, model));
    while service.collect_step().is_continue() {}
    let WmoLoad::Producer(mut producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("new producer".into());
    };
    let request = producer.subscribe_for(CpuService::Required);
    assert!(producer.step(&mut reader)?.is_none());
    drop(producer);
    assert!(matches!(request.poll(), Some(Err(WmoLoadError::Abandoned))));
    Ok(())
}
