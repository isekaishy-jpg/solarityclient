//! Shared texture sources keep exact failures, pending authority and payload lifetime.
use super::texture::raw3_blp;
use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad,
    BlpLoadError, BlpTextureCache, ClientDataRoot, Locale,
};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass as Class,
    CpuStoragePlan, CpuTaskStep,
};
use std::{error::Error, num::NonZeroUsize, sync::Arc};
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/Shared.blp",
        bytes: &raw3_blp(1, 1, &[0xff123456]),
    }])
}
fn catalog(fixture: &Fixture) -> Result<ArchiveCatalog, Box<dyn Error>> {
    Ok(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)
}
fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

#[test]
fn shared_texture_dependency_releases_worker_and_retains_one_payload_charge()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog = catalog(&fixture)?;
    let service = catalog.texture_cache_service();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("Textures/Shared.blp")?);
    let cpu = pool()?;
    catalog
        .model_cache_service()
        .configure_storage(cpu.storage().clone())?;
    let BlpLoad::Producer(producer) = service.request_for(&key, CpuService::Speculative) else {
        return Err("producer".into());
    };
    let BlpLoad::Pending(request) = service.request_for(&key, CpuService::Required) else {
        return Err("join".into());
    };
    let dependency = request.dependency(cpu.storage(), Class::Required)?;
    let mut edge = Some(dependency.task_dependency()?);
    let consumer = cpu.try_reserve()?.submit_resumable_with_context(move |_| {
        if let Some(edge) = edge.take() {
            return CpuTaskStep::Wait(edge);
        }
        CpuTaskStep::Complete(dependency.poll())
    });
    assert_eq!(cpu.try_submit(|| 41)?.join()?, 41);
    let mut reader = AssetStore::mount(catalog.clone())?;
    let policy = AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Speculative);
    let source = cpu
        .try_submit(move || producer.load(&mut reader, &policy))?
        .join()??;
    let received = consumer.join()?.ok_or("published")??;
    assert_eq!(
        cpu.storage().snapshot().used(Class::Speculative),
        source.resident_bytes()
    );
    assert_eq!(received.decode_mip(0)?.rgba8(), &[0x12, 0x34, 0x56, 0xff]);
    let mut a = BlpTextureCache::new();
    let mut b = BlpTextureCache::new();
    let mut reader = AssetStore::mount(catalog)?;
    let first = a.load(&mut reader, key.path())?;
    let second = b.load(&mut reader, key.path())?;
    assert_eq!(cpu.storage().snapshot().used(Class::Speculative), 0);
    assert_eq!(
        cpu.storage()
            .snapshot()
            .bytes(Class::Required, solarity_cpu::CpuStorageKind::Result),
        source.resident_bytes(),
        "independent cache owners share the same allocation"
    );
    drop((first, second));
    assert_eq!(
        a.collect_unused(),
        1,
        "another cache does not pin this cache's entry"
    );
    assert_eq!(b.collect_unused(), 1);
    assert!(
        matches!(
            service.request_for(&key, CpuService::Required),
            BlpLoad::Ready(_)
        ),
        "worker snapshots retain ready payloads after cache collection"
    );
    drop((source, received, request));
    service.collect_unused();
    assert_eq!(
        cpu.storage()
            .snapshot()
            .bytes(Class::Required, solarity_cpu::CpuStorageKind::Result),
        0
    );
    assert!(matches!(
        service.request_for(&key, CpuService::Required),
        BlpLoad::Producer(_)
    ));
    Ok(())
}

#[test]
fn shared_texture_failure_abandonment_and_namespace_preserve_retry() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let first = catalog(&fixture)?;
    let service = first.texture_cache_service();
    let cpu = pool()?;
    let policy = AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
    let mut reader = AssetStore::mount(first.clone())?;
    let missing = AssetResourceKey::new(first.namespace(), AssetPath::new("Textures/Missing.blp")?);
    let BlpLoad::Producer(producer) = service.request_for(&missing, CpuService::Required) else {
        return Err("producer".into());
    };
    let request = producer.subscribe_for(CpuService::Required);
    let edge = request.dependency(cpu.storage(), Class::Required)?;
    let Err(BlpLoadError::Asset(original)) = producer.load(&mut reader, &policy) else {
        return Err("failure".into());
    };
    let Some(Err(BlpLoadError::Asset(delivered))) = edge.poll() else {
        return Err("shared failure".into());
    };
    assert!(Arc::ptr_eq(&original, &delivered));
    let BlpLoad::Producer(producer) = service.request_for(&missing, CpuService::Required) else {
        return Err("retry".into());
    };
    let abandoned = producer.subscribe_for(CpuService::Required);
    let edge = abandoned.dependency(cpu.storage(), Class::Required)?;
    drop(producer);
    assert!(matches!(edge.poll(), Some(Err(BlpLoadError::Abandoned))));
    let key = AssetResourceKey::new(first.namespace(), AssetPath::new("Textures/Shared.blp")?);
    let BlpLoad::Producer(producer) = service.request_for(&key, CpuService::Required) else {
        return Err("source producer".into());
    };
    let mut other = AssetStore::mount(catalog(&fixture)?)?;
    assert!(matches!(
        producer.load(&mut other, &policy),
        Err(BlpLoadError::Namespace { .. })
    ));
    assert!(matches!(
        service.request_for(&key, CpuService::Required),
        BlpLoad::Producer(_)
    ));
    Ok(())
}

#[test]
fn concurrent_texture_claims_have_one_producer() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog = catalog(&fixture)?;
    let service = catalog.texture_cache_service();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("Textures/Shared.blp")?);
    let barrier = std::sync::Barrier::new(8);
    let requests = std::thread::scope(|scope| {
        let tasks = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    service.request_for(&key, CpuService::Required)
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().unwrap_or_else(|_| panic!("claim panicked")))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        requests
            .iter()
            .filter(|request| matches!(request, BlpLoad::Producer(_)))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| matches!(request, BlpLoad::Pending(_)))
            .count(),
        7
    );
    Ok(())
}

/// Replaying a private builder retains exact failed prefixes and suspends without a worker.
#[test]
fn texture_construction_replays_exact_failure_then_shared_source() -> Result<(), Box<dyn Error>> {
    use solarity_asset::{AssetError, BlpTexturePreparation};
    use std::{ops::ControlFlow, sync::mpsc, time::Duration};
    let fixture = fixture()?;
    let catalog = catalog(&fixture)?;
    let cpu = pool()?;
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("Textures/Shared.blp")?);
    let BlpLoad::Producer(producer) = catalog
        .texture_cache_service()
        .request_for(&key, CpuService::Required)
    else {
        return Err("producer".into());
    };
    let mut reader = AssetStore::mount(catalog.clone())?;
    let mut cache = BlpTextureCache::new();
    let mut preparation = BlpTexturePreparation::default();
    let permit = cpu.try_reserve()?;
    let service = permit.service_control();
    let budget = cpu.storage().clone();
    let mut original = None;
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |_| {
        let result = preparation.run(
            &mut cache,
            &mut reader,
            &budget,
            &service,
            |cache, reader| {
                let Err(AssetError::TextureRequest(BlpLoadError::Asset(error))) =
                    cache.load(reader, &AssetPath::new("Textures/Missing.blp")?)
                else {
                    panic!("authored failure stays shared");
                };
                assert!(!error.is_source_pipeline_error());
                if let Some(first) = &original {
                    assert!(Arc::ptr_eq(first, &error));
                } else {
                    original = Some(error);
                }
                cache.load(reader, key.path())
            },
        );
        match result {
            Ok(ControlFlow::Continue(edge)) => {
                let _ = notice.send(());
                CpuTaskStep::Wait(edge)
            }
            Ok(ControlFlow::Break(source)) => CpuTaskStep::Complete(Ok::<_, AssetError>(source)),
            Err(error) => CpuTaskStep::Complete(Err(error)),
        }
    });
    observed.recv_timeout(Duration::from_secs(5))?;
    assert!(!task.is_finished());
    assert_eq!(cpu.try_submit(|| 29)?.join()?, 29);
    let mut reader = AssetStore::mount(catalog)?;
    let policy = AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
    let source = cpu
        .try_submit(move || producer.load(&mut reader, &policy))?
        .join()??;
    let result = task.join()??;
    assert_eq!(result.decode_mip(0)?.rgba8(), source.decode_mip(0)?.rgba8());
    Ok(())
}

/// A dropped producer is a pipeline failure; its consumer can then reuse the cache normally.
#[test]
fn texture_construction_abandonment_and_unwind_restore_cache() -> Result<(), Box<dyn Error>> {
    use solarity_asset::{AssetError, BlpTexturePreparation};
    use std::ops::ControlFlow;
    let fixture = fixture()?;
    let catalog = catalog(&fixture)?;
    let cpu = pool()?;
    let permit = cpu.try_reserve()?;
    let service = permit.service_control();
    let mut reader = AssetStore::mount(catalog.clone())?;
    let mut cache = BlpTextureCache::new();
    let mut preparation = BlpTexturePreparation::default();
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("Textures/Shared.blp")?);
    let BlpLoad::Producer(producer) = catalog
        .texture_cache_service()
        .request_for(&key, CpuService::Required)
    else {
        return Err("producer".into());
    };
    let ControlFlow::Continue(edge) = preparation.run(
        &mut cache,
        &mut reader,
        cpu.storage(),
        &service,
        |cache, reader| cache.load(reader, key.path()),
    )?
    else {
        return Err("suspension".into());
    };
    drop(edge);
    drop(producer);
    let Err(error @ AssetError::TextureRequest(BlpLoadError::Abandoned)) = preparation.run(
        &mut cache,
        &mut reader,
        cpu.storage(),
        &service,
        |cache, reader| cache.load(reader, key.path()),
    ) else {
        return Err("original abandonment".into());
    };
    assert!(error.is_source_pipeline_error());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<ControlFlow<(), _>, AssetError> =
            preparation.run(&mut cache, &mut reader, cpu.storage(), &service, |_, _| {
                panic!("private builder panic")
            });
    }));
    assert!(panic.is_err());
    assert!(cache.load(&mut reader, key.path()).is_ok());
    Ok(())
}
