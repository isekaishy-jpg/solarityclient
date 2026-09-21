//! Fixed effect slots preserve stock failures while shared prerequisites release workers.

use super::{PATHS, prepare};
use crate::application::texture_source_job::SharedTextureSources;
use crate::test_support::{ClientFixture, bootstrap_texture_blp};
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad,
    BlpLoadError, ClientDataRoot, Locale,
};
use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan, CpuTaskStep,
};
use solarity_ui::FontError;
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, mpsc},
    time::Duration,
};

fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(16 << 20, 16 << 20, 16 << 20),
    ))?)
}

#[test]
fn startup_presentation_preserves_fixed_slots_and_only_authored_fallbacks()
-> Result<(), Box<dyn Error>> {
    let blp = bootstrap_texture_blp();
    // The middle slot is corrupt, the last absent: neither changes splash identity.
    let fixture =
        ClientFixture::with_common_files(&[(PATHS[0], &blp), (PATHS[1], b"invalid BLP")])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let result = permit
        .submit_resumable_with_context(prepare(catalog.clone(), shared, (2560, 1440)))
        .join()??;
    assert_eq!(
        result.effects[0].as_ref().ok_or("splash")?.path(),
        &AssetPath::new(PATHS[0])?
    );
    assert!(result.effects[1].is_none());
    assert!(result.effects[2].is_none());
    assert!(
        result.fps.is_none(),
        "missing optional font keeps overlay absent"
    );
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new(PATHS[0])?);
    assert!(matches!(
        catalog
            .texture_cache_service()
            .request_for(&key, CpuService::Required),
        BlpLoad::Ready(_)
    ));
    Ok(())
}

#[test]
fn startup_presentation_shared_source_publish_abandon_cancel_and_admission_error()
-> Result<(), Box<dyn Error>> {
    for action in 0..4 {
        let blp = bootstrap_texture_blp();
        let fixture = ClientFixture::with_common_files(&[
            (PATHS[0], &blp),
            (PATHS[1], &blp),
            (PATHS[2], &blp),
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new(PATHS[1])?);
        let BlpLoad::Producer(producer) = catalog
            .texture_cache_service()
            .request_for(&key, CpuService::Speculative)
        else {
            return Err("producer".into());
        };
        let witness = producer.subscribe_for(CpuService::Speculative);
        let cpu = pool()?;
        let permit = cpu.try_reserve_for(CpuService::Required)?;
        let shared = SharedTextureSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut operation = prepare(catalog.clone(), shared, (2560, 1440));
        let (notice, observed) = mpsc::channel();
        let task = permit.submit_resumable_with_context(move |context| {
            let step = operation(context);
            if matches!(step, CpuTaskStep::Wait(_)) {
                let _ = notice.send(());
            }
            step
        });
        observed.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(
            cpu.try_submit(|| 42)?.join()?,
            42,
            "waiting source releases the sole worker"
        );
        match action {
            0 => {
                let policy =
                    AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
                cpu.try_submit(move || producer.load(&mut AssetStore::mount(catalog)?, &policy))?
                    .join()??;
                let result = task.join()??;
                for (source, path) in result.effects.iter().zip(PATHS) {
                    assert_eq!(
                        source.as_ref().ok_or("authored slot")?.path(),
                        &AssetPath::new(path)?
                    );
                }
            }
            1 => {
                drop(producer);
                assert!(matches!(
                    task.join()?,
                    Err(FontError::Asset(AssetError::TextureRequest(
                        BlpLoadError::Abandoned
                    )))
                ));
            }
            2 => {
                task.cancel();
                assert!(matches!(
                    task.join()?,
                    Err(FontError::Asset(AssetError::SourceStorage(
                        CpuError::JobCancelled
                    )))
                ));
                assert!(
                    witness.poll().is_none(),
                    "withdrawal leaves the other source owner pending"
                );
                drop(producer);
            }
            _ => {
                producer.fail(BlpLoadError::Asset(Arc::new(AssetError::SourceStorage(
                    CpuError::JobCancelled,
                ))));
                let Err(FontError::Asset(AssetError::TextureRequest(BlpLoadError::Asset(error)))) =
                    task.join()?
                else {
                    return Err("source admission failure must not become a green image".into());
                };
                assert!(matches!(
                    error.as_ref(),
                    AssetError::SourceStorage(CpuError::JobCancelled)
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn startup_presentation_reports_a_present_invalid_font() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[("Fonts/FRIZQT__.TTF", b"invalid font")])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let cpu = pool()?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let result = permit
        .submit_resumable_with_context(prepare(catalog, shared, (2560, 1440)))
        .join()?;
    assert!(matches!(result, Err(FontError::Face { .. })));
    Ok(())
}
