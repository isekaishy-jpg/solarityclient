//! UI source workers preserve stock request order and release workers on shared readiness.

use super::{missing_paths, prepare_sources};
use crate::application::texture_source_job::SharedTextureSources;
use crate::test_support::{ClientFixture, bootstrap_texture_blp};
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad,
    BlpLoadError, BlpTextureCache, ClientDataRoot, Locale,
};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan, CpuTaskStep,
};
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderMask, UiRenderQuad, UiRenderSource, UiTextureAddressMode,
    UiTextureResidency,
};
use solarity_ui::UiTextureAssetPlan;
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

fn plan() -> Result<UiTextureAssetPlan, Box<dyn Error>> {
    let quad = |path, residency| -> Result<_, Box<dyn Error>> {
        Ok(UiRenderQuad::new(
            0,
            UiRenderSource::Texture(AssetPath::new(path)?),
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            residency,
            false,
            [0.0, 0.0, 32.0, 32.0],
            [[0.0; 2]; 4],
            [[1.0; 4]; 4],
        ))
    };
    Ok(UiTextureAssetPlan::prepare(&UiMeshPlan::prepare(
        [64.0; 2],
        [
            quad("tile.blp", UiTextureResidency::Blocking)?.with_mask(UiRenderMask::new(
                AssetPath::new("mask.blp")?,
                [0.0, 0.0, 64.0, 64.0],
            )),
            quad("MASK.BLP", UiTextureResidency::NonBlocking)?,
            quad("missing-nonblocking.blp", UiTextureResidency::NonBlocking)?,
        ]
        .into_iter(),
    )?)?)
}

#[test]
fn ui_sources_keep_mask_order_skip_nonblocking_and_reuse_the_reader() -> Result<(), Box<dyn Error>>
{
    let blp = bootstrap_texture_blp();
    let fixture = ClientFixture::with_common_files(&[("tile.blp", &blp), ("mask.blp", &blp)])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let plan = plan()?;
    let paths = missing_paths(&plan, |_| false);
    assert_eq!(
        paths,
        [AssetPath::new("tile.blp")?, AssetPath::new("mask.blp")?]
    );
    assert_eq!(
        missing_paths(&plan, |path| *path == paths[0]),
        vec![paths[1].clone()]
    );
    assert!(missing_paths(&plan, |_| true).is_empty());
    let mut serial = AssetStore::mount(catalog.clone())?;
    let expected = plan.load_blocking(&mut serial, &mut BlpTextureCache::new())?;
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let prepared = permit
        .submit_resumable_with_context(prepare_sources(
            catalog.clone(),
            None,
            BlpTextureCache::new(),
            paths.clone(),
            shared,
        ))
        .join()?;
    let sources = prepared.sources?;
    assert_eq!(sources.len(), expected.resident_count());
    assert_eq!(expected.pending_count(), 1);
    for (index, source) in sources.iter().enumerate() {
        let expected = expected.source(index).ok_or("serial blocking source")?;
        assert_eq!(source.path(), expected.path());
        assert_eq!(
            source.decode_mip(0)?.rgba8(),
            expected.decode_mip(0)?.rgba8()
        );
    }
    let identity = prepared.reader.as_ref().ok_or("reader")?.identity();
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let prepared = permit
        .submit_resumable_with_context(prepare_sources(
            catalog,
            prepared.reader,
            prepared.cache,
            paths,
            shared,
        ))
        .join()?;
    assert_eq!(
        prepared.reader.as_ref().ok_or("reused reader")?.identity(),
        identity
    );
    let reused = prepared.sources?;
    assert!(std::sync::Arc::ptr_eq(&sources[0], &reused[0]));
    assert_eq!(prepared.cache.len(), 2);
    Ok(())
}

#[test]
fn ui_sources_join_publish_abandon_and_withdraw_without_occupying_the_worker()
-> Result<(), Box<dyn Error>> {
    for action in [0, 1, 2] {
        let blp = bootstrap_texture_blp();
        let fixture = ClientFixture::with_common_files(&[("tile.blp", &blp), ("mask.blp", &blp)])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let paths = missing_paths(&plan()?, |_| false);
        let key = AssetResourceKey::new(catalog.namespace(), paths[1].clone());
        let service = catalog.texture_cache_service();
        let BlpLoad::Producer(producer) = service.request_for(&key, CpuService::Speculative)?
        else {
            return Err("producer".into());
        };
        let witness = producer.subscribe_for(CpuService::Speculative)?;
        let cpu = pool()?;
        let permit = cpu.try_reserve_for(CpuService::Required)?;
        let shared = SharedTextureSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut operation =
            prepare_sources(catalog.clone(), None, BlpTextureCache::new(), paths, shared);
        let (notice, observed) = mpsc::channel();
        let task = permit.submit_resumable_with_context(move |context| {
            let step = operation(context);
            if matches!(step, CpuTaskStep::Wait(_)) {
                let _ = notice.send(());
            }
            step
        });
        observed.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
        let prepared = match action {
            0 => {
                let policy =
                    AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
                cpu.try_submit(move || producer.load(&mut AssetStore::mount(catalog)?, &policy))?
                    .join()??;
                let prepared = task.join()?;
                assert_eq!(prepared.sources.as_ref().map(Vec::len).ok(), Some(2));
                prepared
            }
            1 => {
                drop(producer);
                let prepared = task.join()?;
                assert!(matches!(
                    prepared.sources,
                    Err(AssetError::TextureRequest(BlpLoadError::Abandoned))
                ));
                prepared
            }
            _ => {
                task.cancel();
                let prepared = task.join()?;
                assert!(matches!(
                    prepared.sources,
                    Err(AssetError::SourceStorage(
                        solarity_cpu::CpuError::JobCancelled
                    ))
                ));
                assert!(
                    witness.poll().is_none(),
                    "consumer withdrawal does not fail another producer"
                );
                drop(producer);
                prepared
            }
        };
        assert!(prepared.reader.is_some());
        assert_eq!(prepared.cache.len(), if action == 0 { 2 } else { 1 });
    }
    Ok(())
}

#[test]
fn ui_sources_report_the_first_authored_error_before_a_later_pending_source()
-> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[("mask.blp", &bootstrap_texture_blp())])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let paths = missing_paths(&plan()?, |_| false);
    let key = AssetResourceKey::new(catalog.namespace(), paths[1].clone());
    let BlpLoad::Producer(_producer) = catalog
        .texture_cache_service()
        .request_for(&key, CpuService::Speculative)?
    else {
        return Err("producer".into());
    };
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let prepared = permit
        .submit_resumable_with_context(prepare_sources(
            catalog,
            None,
            BlpTextureCache::new(),
            paths,
            shared,
        ))
        .join()?;
    let Err(AssetError::TextureRequest(BlpLoadError::Asset(error))) = prepared.sources else {
        return Err("expected first missing source".into());
    };
    assert!(
        matches!(error.as_ref(), AssetError::AssetNotFound { path } if *path == AssetPath::new("tile.blp")?)
    );
    assert!(prepared.reader.is_some());
    assert!(prepared.cache.is_empty());
    Ok(())
}
