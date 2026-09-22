//! Cancellation and producer failure use the actual effect-service continuation.

use super::*;
use crate::test_support::{ClientFixture, unit_models};
use solarity_asset::{
    AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, M2Load, M2LoadProducer,
};
use solarity_cpu::{CpuExecutionPlan, CpuPoolConfig, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

type EffectSourceTask = CpuTask<Result<PreparedUnitEffects, RuntimeTerrainError>>;

/// Retain fixture files until every mounted reader and shared producer has returned.
fn fixture() -> Result<(ClientFixture, ArchiveCatalog, M2LoadProducer, CpuExecutor), Box<dyn Error>>
{
    let fixture = unit_models::fixture_with_water_effects(17)?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("World/WaterEffect.m2")?);
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("shared effect producer".into());
    };
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    Ok((fixture, catalog, producer, cpu))
}

/// Observe the declared wait without modifying the domain operation or relying
/// on wall-clock guesses about which archive/decoder stage has run.
fn waiting_task(
    cpu: &CpuExecutor,
    catalog: ArchiveCatalog,
) -> Result<EffectSourceTask, Box<dyn Error>> {
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut operation = source_steps(
        catalog,
        Arc::new(EnvironmentalDamageCatalog::default()),
        shared,
    );
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |context| {
        let result = operation(context);
        if matches!(result, CpuTaskStep::Wait(_)) {
            let _ = notice.send(());
        }
        result
    });
    observed.recv_timeout(Duration::from_secs(5))?;
    Ok(task)
}

/// Withdrawing one effect bank cannot destroy the source used by other consumers.
#[test]
fn cancelled_effect_bank_releases_its_dependency_without_abandoning_the_source()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, producer, mut cpu) = fixture()?;
    let task = waiting_task(&cpu, catalog.clone())?;
    task.cancel();
    assert!(matches!(
        task.join()?,
        Err(RuntimeTerrainError::Cpu(
            solarity_cpu::CpuError::JobCancelled
        ))
    ));
    assert_eq!(cpu.try_submit(|| 41)?.join()?, 41);
    let mut reader = AssetStore::mount(catalog.clone())?;
    let model = producer.load(&mut reader)?;
    let key = AssetResourceKey::new(catalog.namespace(), model.path().clone());
    assert!(matches!(
        catalog.model_cache_service().request(&key)?,
        M2Load::Ready(_)
    ));
    cpu.shutdown()?;
    Ok(())
}

/// Pipeline abandonment is terminal, rather than silently substituting an empty bank.
#[test]
fn abandoned_effect_producer_wakes_the_bank_and_returns_its_original_failure()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, producer, mut cpu) = fixture()?;
    let task = waiting_task(&cpu, catalog)?;
    drop(producer);
    assert!(matches!(
        task.join()?,
        Err(RuntimeTerrainError::SharedModel(
            solarity_asset::M2LoadError::Abandoned
        ))
    ));
    cpu.shutdown()?;
    Ok(())
}

/// Namespace metadata refusal must fail the bank before decoding, never omit authored effects.
#[test]
fn effect_source_storage_refusal_preserves_the_pipeline_failure() -> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture_with_water_effects(17)?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let denied = solarity_cpu::CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 0));
    catalog
        .model_cache_service()
        .configure_storage(denied.clone())?;
    // Fixed namespace controls must exist before the worker can reach a refused request.
    let snapshot = denied.snapshot();
    let class = solarity_cpu::CpuStorageClass::Required;
    let _pressure = denied.reserve(
        class,
        solarity_cpu::CpuStorageKind::Scratch,
        snapshot.limit(class) - snapshot.used(class),
    )?;
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let task = permit.submit_resumable_with_context(source_steps(
        catalog,
        Arc::new(EnvironmentalDamageCatalog::default()),
        shared,
    ));
    assert!(matches!(
        task.join()?,
        Err(RuntimeTerrainError::Asset(
            solarity_asset::AssetError::SourceStorage(
                solarity_cpu::CpuError::StorageAtCapacity { .. }
            )
        ))
    ));
    assert_eq!(
        cpu.storage().snapshot().bytes(
            solarity_cpu::CpuStorageClass::Required,
            solarity_cpu::CpuStorageKind::Result
        ),
        0
    );
    Ok(())
}
