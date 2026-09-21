//! Startup transfers one complete namespace/catalog bank and never publishes a prefix.

use super::{LOADS, StartupCatalogError, prepare};
use crate::application::texture_source_job::SharedTextureSources;
use crate::test_support::ClientFixture;
use solarity_asset::{ArchiveCatalog, AssetError, ClientDataRoot, Locale};
use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass,
    CpuStorageKind, CpuStoragePlan, CpuTaskDependency, CpuTaskStep, SharedProduct,
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

fn pool(capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(capacity).ok_or("capacity")?,
        CpuStoragePlan::new(16 << 20, 16 << 20, 16 << 20),
    ))?)
}

fn startup_fixture() -> Result<ClientFixture, Box<dyn Error>> {
    let mut displays = b"WDBC".to_vec();
    for field in [0_u32, 19, 76, 1] {
        displays.extend_from_slice(&field.to_le_bytes());
    }
    displays.push(0);
    ClientFixture::with_common_files(&[("DBFilesClient/GameObjectDisplayInfo.dbc", &displays)])
}

#[test]
fn startup_catalogs_complete_on_one_worker_and_one_admitted_slot() -> Result<(), Box<dyn Error>> {
    let fixture = startup_fixture()?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let archives = ArchiveCatalog::discover(root.clone(), Locale::EnUs)?
        .descriptors()
        .len();
    let cpu = pool(1)?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut operation = prepare(root, Locale::EnUs, shared, (1280, 720));
    let calls = Arc::new(AtomicUsize::new(0));
    let worker_calls = Arc::clone(&calls);
    let mut prepared = permit
        .submit_resumable_with_context(move |context| {
            worker_calls.fetch_add(1, Ordering::Relaxed);
            operation(context)
        })
        .join()??;
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1 + archives + LOADS.len() + 1 + 4
    );
    assert_eq!(prepared.catalog.namespace(), prepared.assets.namespace());
    assert_eq!(prepared.assets.archives().len(), archives);
    let serial_realms =
        crate::application::realm_directory::RuntimeRealmMetadata::load(&mut prepared.assets)?;
    assert_eq!(
        prepared.catalogs.realm_metadata.empty_directory(),
        serial_realms.empty_directory()
    );
    assert_eq!(
        prepared.catalogs.addon_catalog,
        solarity_ui::AddonCatalog::discover(&mut prepared.assets)?
    );
    assert_eq!(
        prepared.addon_manifest.addons().len(),
        prepared.catalogs.addon_catalog.addons().len()
    );
    let character = prepared.catalogs.character_metadata.publish();
    assert!(std::rc::Rc::ptr_eq(
        &character.faction_catalog(),
        &character.faction_catalog()
    ));
    assert!(
        prepared.catalogs.loading_screens.is_none(),
        "missing optional LoadingScreens retains stock startup policy"
    );
    assert!(prepared.presentation?.fps.is_none());
    Ok(())
}

#[test]
fn startup_catalogs_report_first_table_failure_and_keep_font_failure_separate()
-> Result<(), Box<dyn Error>> {
    for bad_table in [false, true] {
        let fixture = startup_fixture()?;
        let mut patch = wow_mpq::ArchiveBuilder::new()
            .add_file_data(b"invalid font".to_vec(), "Fonts/FRIZQT__.TTF");
        if bad_table {
            patch = patch
                .add_file_data(
                    b"invalid first table".to_vec(),
                    "DBFilesClient/EnvironmentalDamage.dbc",
                )
                .add_file_data(
                    b"invalid later table".to_vec(),
                    "DBFilesClient/AnimationData.dbc",
                );
        }
        patch.build(fixture.data_root().join("patch.MPQ"))?;
        let cpu = pool(1)?;
        let permit = cpu.try_reserve_for(CpuService::Required)?;
        let shared = SharedTextureSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let result = permit
            .submit_resumable_with_context(prepare(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
                shared,
                (1280, 720),
            ))
            .join()?;
        if bad_table {
            let Err(StartupCatalogError::Asset(AssetError::DatabaseDecode { path, .. })) = result
            else {
                return Err("first catalog failure must precede later table/font failures".into());
            };
            assert_eq!(
                path,
                solarity_asset::AssetPath::new("DBFilesClient/EnvironmentalDamage.dbc")?
            );
        } else {
            assert!(matches!(
                result?.presentation,
                Err(solarity_ui::FontError::Face { .. })
            ));
        }
    }
    Ok(())
}

#[test]
fn startup_catalog_read_pressure_is_fatal_before_decode() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let cpu = pool(1)?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let snapshot = cpu.storage().snapshot();
    let _pressure = cpu.storage().reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        snapshot.limit(CpuStorageClass::Required) - snapshot.used(CpuStorageClass::Required),
    )?;
    let result = permit
        .submit_resumable_with_context(prepare(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
            shared,
            (1280, 720),
        ))
        .join()?;
    assert!(matches!(
        result,
        Err(StartupCatalogError::Asset(AssetError::ReadAdmission { .. }))
    ));
    Ok(())
}

#[test]
fn startup_catalog_cancellation_retires_an_unpublished_prefix() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let archives = ArchiveCatalog::discover(root.clone(), Locale::EnUs)?
        .descriptors()
        .len();
    let cpu = pool(8)?;
    let (producer, product) =
        SharedProduct::<(), ()>::new(1, cpu.storage(), CpuStorageClass::Required)?;
    let mut edge = Some(CpuTaskDependency::new(&product.readiness())?);
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut operation = prepare(root, Locale::EnUs, shared, (1280, 720));
    let mut calls = 0;
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |context| {
        if calls == 1 + archives + 3
            && !context.is_cancelled()
            && let Some(edge) = edge.take()
        {
            let _ = notice.send(());
            return CpuTaskStep::Wait(edge);
        }
        calls += 1;
        operation(context)
    });
    observed.recv_timeout(Duration::from_secs(5))?;
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    task.cancel();
    assert!(matches!(
        task.join()?,
        Err(StartupCatalogError::Asset(AssetError::SourceStorage(
            CpuError::JobCancelled
        )))
    ));
    producer.publish(Ok(()));
    assert_eq!(cpu.try_submit(|| 43)?.join()?, 43);
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT containing build-12340 client archives"]
fn stock_startup_catalogs_and_presentation_complete_on_one_slot() -> Result<(), Box<dyn Error>> {
    let root = ClientDataRoot::new(
        std::env::var_os("SOLARITY_STOCK_DATA_ROOT")
            .ok_or("SOLARITY_STOCK_DATA_ROOT must be set")?,
    )?;
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(1).ok_or("capacity")?,
        CpuStoragePlan::new(128 << 20, 256 << 20, 64 << 20),
    ))?;
    let permit = cpu.try_reserve_for(CpuService::Required)?;
    let shared = SharedTextureSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut prepared = permit
        .submit_resumable_with_context(prepare(root, Locale::EnUs, shared, (2560, 1440)))
        .join()??;
    assert_eq!(prepared.catalog.namespace(), prepared.assets.namespace());
    assert_eq!(
        prepared.catalogs.realm_metadata.empty_directory(),
        crate::application::realm_directory::RuntimeRealmMetadata::load(&mut prepared.assets)?
            .empty_directory()
    );
    assert_eq!(
        prepared.catalogs.addon_catalog,
        solarity_ui::AddonCatalog::discover(&mut prepared.assets)?
    );
    assert!(prepared.catalogs.loading_screens.is_some());
    assert!(prepared.presentation?.fps.is_some());
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    Ok(())
}
