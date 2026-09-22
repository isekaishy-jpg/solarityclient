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
    let displays = empty_table(19);
    let lookups = empty_table(3);
    ClientFixture::with_common_files(&[
        ("DBFilesClient/GameObjectDisplayInfo.dbc", &displays),
        ("DBFilesClient/UISoundLookups.dbc", &lookups),
    ])
}

fn empty_table(fields: u32) -> Vec<u8> {
    let mut displays = b"WDBC".to_vec();
    for field in [0_u32, fields, fields * 4, 1] {
        displays.extend_from_slice(&field.to_le_bytes());
    }
    displays.push(0);
    displays
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
        1 + archives + LOADS.len() + 1 + 4 + 2
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
    let ui = prepared.ui?;
    ui.sound.engine?;
    ui.sound.world?;
    drop(cpu);
    let assets = solarity_asset::AssetStoreHandle::new(prepared.assets);
    let random = || {
        std::rc::Rc::new(std::cell::RefCell::new(solarity_cpu::BlizzardRand::new(
            123,
        )))
    };
    let serial_random = random();
    let worker_random = random();
    let serial = solarity_ui::GlueManager::start_shared_with_profile_and_random(
        assets.clone(),
        (1280, 720),
        false,
        solarity_ui::GlueInitialScreen::Login,
        &[],
        &prepared.catalogs.addon_catalog,
        serial_random.clone(),
    )?;
    let worker = solarity_ui::GlueManager::start_shared_with_sources(
        assets,
        (1280, 720),
        solarity_ui::GlueInitialScreen::Login,
        &[],
        &prepared.catalogs.addon_catalog,
        worker_random.clone(),
        ui.glue,
        solarity_ui::FontSystem::new()?,
    )?;
    assert_eq!(worker.report(), serial.report());
    assert_eq!(worker.objects(), serial.objects());
    assert!(worker.bundle().lua().globals().get::<bool>("GLUE_READY")?);
    assert_eq!(
        worker_random.borrow_mut().next_u32(),
        serial_random.borrow_mut().next_u32()
    );
    Ok(())
}

#[test]
fn startup_glue_and_sound_errors_stay_at_their_consumption_boundaries() -> Result<(), Box<dyn Error>>
{
    for bad_glue in [false, true] {
        let fixture = startup_fixture()?;
        let mut patch = wow_mpq::ArchiveBuilder::new()
            .add_file_data(b"bad sound".to_vec(), "DBFilesClient/SoundEntries.dbc")
            .add_file_data(
                b"bad movement".to_vec(),
                "DBFilesClient/CreatureSoundData.dbc",
            );
        if bad_glue {
            patch = patch
                .add_file_data(b"bad creation".to_vec(), "DBFilesClient/CharBaseInfo.dbc")
                .add_file_data(b"bad xml".to_vec(), "Interface/GlueXML/Bootstrap.xml");
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
            .join()??;
        result.presentation?;
        if bad_glue {
            let Err(solarity_ui::GlueError::Asset(AssetError::DatabaseDecode { path, .. })) =
                result.ui
            else {
                return Err("creation metadata must fail before Glue XML".into());
            };
            assert_eq!(
                path,
                solarity_asset::AssetPath::new("DBFilesClient/CharBaseInfo.dbc")?
            );
        } else {
            let sound = result.ui?.sound;
            let Err(AssetError::DatabaseDecode { path, .. }) = sound.engine else {
                return Err("engine source failure must remain deferred".into());
            };
            assert_eq!(
                path,
                solarity_asset::AssetPath::new("DBFilesClient/SoundEntries.dbc")?
            );
            let Err(AssetError::DatabaseDecode { path, .. }) = sound.world else {
                return Err("world sound source failure must remain separate".into());
            };
            assert_eq!(
                path,
                solarity_asset::AssetPath::new("DBFilesClient/CreatureSoundData.dbc")?
            );
        }
    }
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
    let budget = cpu.storage().clone();
    let mut pressure = None;
    let mut operation = prepare(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
        shared,
        (1280, 720),
    );
    let result = permit
        .submit_resumable_with_context(move |context| {
            let step = operation(context);
            if pressure.is_none() && matches!(step, CpuTaskStep::Continue) {
                // Discovery has admitted namespace controls. Exhaust only the remaining
                // headroom so this fixture still reaches the first encoded table read.
                let snapshot = budget.snapshot();
                assert!(snapshot.bytes(CpuStorageClass::Required, CpuStorageKind::Metadata) != 0);
                match budget.reserve(
                    CpuStorageClass::Required,
                    CpuStorageKind::Scratch,
                    snapshot.limit(CpuStorageClass::Required)
                        - snapshot.used(CpuStorageClass::Required),
                ) {
                    Ok(memory) => pressure = Some(memory),
                    Err(error) => {
                        return CpuTaskStep::Complete(Err(AssetError::from(error).into()));
                    }
                }
            }
            step
        })
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
    let ui = prepared.ui?;
    ui.sound.engine?;
    ui.sound.world?;
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    Ok(())
}
