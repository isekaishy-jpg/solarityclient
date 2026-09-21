//! Nested sources suspend a real single worker and preserve shared generation identity.

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale, M2ModelCache};
use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, CpuTaskStep};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

#[test]
fn nested_appearance_sources_suspend_share_and_skip_missing_attachment_links()
-> Result<(), Box<dyn Error>> {
    let bytes = game_object_models::model()?;
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("Child.m2", &bytes),
        ("Child00.skin", &skin),
        ("Next.m2", &bytes),
        ("Next00.skin", &skin),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let path = AssetPath::new("Child.m2")?;
    let key = AssetResourceKey::new(catalog.namespace(), path.clone());
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("one external producer".into());
    };
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let mut entries = CpuBuffer::default();
    let mut models = CpuBuffer::default();
    entries.reserve(
        cpu.storage(),
        CpuStorageClass::Required,
        CpuStorageKind::Metadata,
        4,
    )?;
    models.reserve(
        cpu.storage(),
        CpuStorageClass::Required,
        CpuStorageKind::Result,
        4,
    )?;
    for entry in [
        SourceEntry {
            path: path.clone(),
            parent: None,
        },
        SourceEntry {
            path: AssetPath::new("MissingEffect.m2")?,
            parent: Some((0, u32::MAX)),
        },
        SourceEntry {
            path: AssetPath::new("Child.mdx")?,
            parent: None,
        },
        SourceEntry {
            path: AssetPath::new("Next.m2")?,
            parent: None,
        },
    ] {
        entries.push(entry)?;
    }
    let mut work = SourceWork {
        entries,
        models,
        pending: None,
    };

    let mut store = AssetStore::mount(catalog.clone())?;
    let permit = cpu.try_reserve()?;
    let control = permit.service_control();
    let budget = cpu.storage().clone();
    let (suspended, waiting) = mpsc::channel();
    let (completed, completion) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |_| {
        match work.step(&mut store, &budget, &control) {
            Ok(ControlFlow::Continue(Some(edge))) => {
                let _ = suspended.send(());
                CpuTaskStep::Wait(edge)
            }
            Ok(ControlFlow::Continue(None)) => CpuTaskStep::Continue,
            Ok(ControlFlow::Break(())) => {
                assert_eq!(
                    control.service(),
                    solarity_cpu::CpuService::Required,
                    "a completed nested producer cannot withdraw the required appearance"
                );
                let _ = completed.send(());
                CpuTaskStep::Complete(Ok(std::mem::take(&mut work.models)))
            }
            Err(error) => CpuTaskStep::Complete(Err(error)),
        }
    });
    waiting.recv_timeout(Duration::from_secs(10))?;
    assert_eq!(
        cpu.try_submit(|| 17)?.join()?,
        17,
        "a nested dependency releases the sole worker"
    );
    assert!(!task.is_finished());
    let shared = producer.load(&mut AssetStore::mount(catalog.clone())?)?;
    completion.recv_timeout(Duration::from_secs(10))?;
    let models = task.join()??;
    assert!(ResourceLease::ptr_eq(
        models[0].as_ref().ok_or("first child")?,
        &shared
    ));
    assert!(
        models[1].is_none(),
        "an absent authored link never requests a missing effect"
    );
    assert!(ResourceLease::ptr_eq(
        models[2].as_ref().ok_or("canonical child")?,
        &shared
    ));
    let mut local = M2ModelCache::new();
    let loaded = local.load(&mut AssetStore::mount(catalog)?, &path)?;
    assert!(
        ResourceLease::ptr_eq(&loaded, &shared),
        "derived construction uses the pinned namespace source"
    );
    assert!(
        local.is_empty(),
        "shared inputs do not populate a second retained source cache"
    );
    Ok(())
}
