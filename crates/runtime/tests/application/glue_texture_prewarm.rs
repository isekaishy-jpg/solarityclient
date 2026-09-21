//! Source stepping preserves aliases, payloads and the existing diagnostic order.

use crate::application::texture_source_job::SharedTextureSources;
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan, CpuTaskStep,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use solarity_asset::{ArchiveCatalog, AssetPath, ClientDataRoot, Locale};

use super::prepare_configured_glue_textures;
use crate::test_support::{ClientFixture, bootstrap_texture_blp};

/// Mounts and each requested path have separate turns; no entry is skipped or retried.
#[test]
fn configured_texture_steps_keep_alias_reuse_and_ordered_failures() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[("fixture.blp", &bootstrap_texture_blp())])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mounts = catalog.descriptors().len();
    let namespace = catalog.namespace();
    let paths = [
        "fixture.blp",
        "missing-one.blp",
        "FIXTURE.BLP",
        "missing-two.blp",
    ]
    .map(AssetPath::new)
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Speculative)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut operation = prepare_configured_glue_textures(catalog, paths, shared);
    let steps = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&steps);
    let result = permit
        .submit_resumable_with_context(move |context| {
            count.fetch_add(1, Ordering::Relaxed);
            operation(context)
        })
        .join()??;
    assert_eq!(steps.load(Ordering::Relaxed), 1 + mounts + 4 + 1);
    assert_eq!(result.cache.len(), 1);
    assert_eq!(result.cache.entries(namespace).count(), 1);
    assert_eq!(result.failures.len(), 2);
    assert!(result.failures[0].contains("MISSING-ONE.BLP"));
    assert!(result.failures[1].contains("MISSING-TWO.BLP"));
    Ok(())
}

fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

#[test]
fn configured_texture_join_releases_worker_and_cancellation_keeps_other_consumers()
-> Result<(), Box<dyn Error>> {
    use solarity_asset::{AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad, BlpLoadError};
    let fixture = ClientFixture::with_common_files(&[("fixture.blp", &bootstrap_texture_blp())])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let path = AssetPath::new("fixture.blp")?;
    let key = AssetResourceKey::new(catalog.namespace(), path.clone());
    let service = catalog.texture_cache_service();
    let BlpLoad::Producer(producer) = service.request_for(&key, CpuService::Required)? else {
        return Err("producer".into());
    };
    let witness = producer.subscribe_for(CpuService::Required)?;
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Speculative)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut operation =
        prepare_configured_glue_textures(catalog.clone(), vec![path.clone()], shared);
    let (suspended, wait) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |context| {
        let step = operation(context);
        if matches!(step, CpuTaskStep::Wait(_)) {
            let _ = suspended.send(());
        }
        step
    });
    wait.recv_timeout(Duration::from_secs(5))?;
    assert_eq!(
        cpu.try_submit(|| 42)?.join()?,
        42,
        "joined work releases the sole worker"
    );
    task.cancel();
    assert!(matches!(
        task.join()?,
        Err(solarity_asset::AssetError::SourceStorage(
            solarity_cpu::CpuError::JobCancelled
        ))
    ));
    assert!(witness.poll().is_none());
    let permit = cpu.try_reserve_for(CpuService::Speculative)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let task = permit.submit_resumable_with_context(prepare_configured_glue_textures(
        catalog.clone(),
        vec![path],
        shared,
    ));
    let mut store = AssetStore::mount(catalog)?;
    let policy = AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
    let source = cpu
        .try_submit(move || producer.load(&mut store, &policy))?
        .join()??;
    let prepared = task.join()??;
    assert!(prepared.failures.is_empty());
    assert_eq!(prepared.cache.len(), 1);
    assert_eq!(
        witness.poll().ok_or("published")??.resident_bytes(),
        source.resident_bytes()
    );
    assert!(!matches!(
        witness.poll(),
        Some(Err(BlpLoadError::Abandoned))
    ));
    Ok(())
}
