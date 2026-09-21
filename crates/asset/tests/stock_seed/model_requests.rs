//! Shared primary-source requests preserve source identity and terminal failures.

use crate::{
    model::{m2_bytes, skin_bytes},
    support::{Fixture, FixtureFile},
};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, M2Load,
    M2LoadError, ResourceLease,
};
use std::{error::Error, sync::Arc};

/// Real model/skin decoding is exercised without local client data or gameplay clocks.
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Solarity/Shared.m2",
            bytes: &m2_bytes("Shared", 1)?,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Solarity/Shared00.skin",
            bytes: &skin_bytes(32, &[0, 1, 2])?,
        },
    ])
}

/// Different catalog consumers and legacy path spellings join the one producer.
#[test]
fn pending_model_consumers_share_exact_decode_and_survive_other_consumer_release()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = sources.request(&key)? else {
        return Err("first request did not own production".into());
    };
    let cancelled = producer.subscribe()?;
    let alias = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.mdx")?,
    );
    let M2Load::Pending(joined) = catalog.clone().model_cache_service().request(&alias)? else {
        return Err("same model was not joined".into());
    };
    assert!(joined.poll().is_none());
    drop(cancelled);
    let model = producer.load(&mut AssetStore::mount(catalog)?)?;
    let delivered = joined.poll().ok_or("producer did not publish")??;
    assert!(ResourceLease::ptr_eq(&model, &delivered));
    let M2Load::Ready(reused) = sources.request(&key)? else {
        return Err("published source was not retained".into());
    };
    assert!(ResourceLease::ptr_eq(&model, &reused));
    Ok(())
}

/// Failures fan out without copying the source error or installing a negative-cache TTL.
#[test]
fn model_request_failure_is_shared_and_abandoned_producer_wakes_consumers()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Missing.m2")?,
    );
    let M2Load::Producer(producer) = sources.request(&key)? else {
        return Err("missing producer".into());
    };
    let joined = producer.subscribe()?;
    let original = producer.load(&mut AssetStore::mount(catalog)?);
    let delivered = joined.poll().ok_or("missing terminal error")?;
    let (Err(M2LoadError::Asset(original)), Err(M2LoadError::Asset(delivered))) =
        (original, delivered)
    else {
        return Err("source failure changed category".into());
    };
    assert!(Arc::ptr_eq(&original, &delivered));
    let M2Load::Producer(producer) = sources.request(&key)? else {
        return Err("failure was persistently cached".into());
    };
    let joined = producer.subscribe()?;
    let worker_consumer = joined.clone();
    let mut cpu = solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        std::num::NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let worker_result = cpu.try_submit(move || worker_consumer.wait())?.join()?;
    assert!(matches!(worker_result, Err(M2LoadError::WorkerWait)));
    cpu.shutdown()?;
    let waiting = std::thread::spawn(move || joined.wait());
    drop(producer);
    assert!(matches!(
        waiting.join().map_err(|_| "waiter panicked")?,
        Err(M2LoadError::Abandoned)
    ));
    Ok(())
}

/// Identical filesystem paths cannot publish a different archive selection under the captured key.
#[test]
fn model_producer_rejects_a_reader_from_another_namespace() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let other = ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("missing producer".into());
    };
    let joined = producer.subscribe()?;
    assert!(matches!(
        producer.load(&mut AssetStore::mount(other)?),
        Err(M2LoadError::Namespace { .. })
    ));
    assert!(matches!(
        joined.poll(),
        Some(Err(M2LoadError::Namespace { .. }))
    ));
    Ok(())
}

/// A selected join changes the real producer queue immediately and withdraws independently.
#[test]
fn required_model_join_promotes_and_release_restores_prewarm() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = sources.request(&key)? else {
        return Err("missing producer".into());
    };
    let prewarm = producer.subscribe_for(solarity_cpu::CpuService::Speculative)?;
    let mut store = AssetStore::mount(catalog)?;
    let mut cpu = solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        // The blocker and speculative producer must leave required admission headroom.
        std::num::NonZeroUsize::new(3).ok_or("positive capacity required")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let task = cpu.try_submit_for(solarity_cpu::CpuService::Speculative, move || {
        producer.load(&mut store)
    })?;
    let control = task.service_control();
    let bound = prewarm.bind_service(control.clone());
    let joined = sources.request(&key)?;
    let required = control.service();
    let was_pending = matches!(joined, M2Load::Pending(_));
    drop(joined);
    let withdrawn = control.service();
    release.send(())?;
    blocker.join()??;
    let model = task.join()??;
    assert!(bound && was_pending);
    assert_eq!(required, solarity_cpu::CpuService::Required);
    assert_eq!(withdrawn, solarity_cpu::CpuService::Speculative);
    assert!(ResourceLease::ptr_eq(
        &model,
        &prewarm.poll().ok_or("missing result")??
    ));
    cpu.shutdown()?;
    Ok(())
}

/// Real decode publication wakes a gated consumer without coordinator polling;
/// registering after publication delivers the same immutable source identity.
#[test]
fn shared_model_readiness_drives_loading_and_late_subscribers() -> Result<(), Box<dyn Error>> {
    use solarity_cpu::{
        CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass, CpuStoragePlan, JobOutcome,
        LoadBatch,
    };
    use std::num::NonZeroUsize;
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe()?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let cancelled = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    let cancelled_ready = cancelled.readiness();
    drop(cancelled);
    assert!(cancelled_ready.outcome().is_err());
    let dependency = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    let ready = dependency.readiness();
    let mut jobs = vec![(dependency, None)];
    let mut load = LoadBatch::new(
        CpuService::Required,
        |work: &mut (
            solarity_asset::M2LoadDependency,
            Option<ResourceLease<solarity_asset::DecodedM2Model>>,
        )| {
            match work.0.poll() {
                Some(Ok(model)) => {
                    work.1 = Some(model);
                    JobOutcome::Succeeded
                }
                _ => JobOutcome::Failed,
            }
        },
    );
    load.start_after(&cpu, &mut jobs, &[ready])?;
    assert_eq!(cpu.try_submit(|| 17)?.join()?, 17);
    let mut reader = AssetStore::mount(catalog)?;
    let model = cpu
        .try_submit(move || producer.load(&mut reader))?
        .join()??;
    load.reclaim(&mut jobs)?;
    assert!(ResourceLease::ptr_eq(
        &model,
        jobs[0].1.as_ref().ok_or("missing derived source")?
    ));
    let late = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    assert_eq!(late.readiness().outcome()?, Some(JobOutcome::Succeeded));
    drop(request);
    cpu.shutdown()?;
    assert!(ResourceLease::ptr_eq(
        &model,
        &late
            .poll()
            .ok_or("missing product after request and executor retirement")??
    ));
    Ok(())
}

/// Producer abandonment fails only its dependent work and retains error identity.
#[test]
fn abandoned_model_dependency_returns_owned_input_without_running_it() -> Result<(), Box<dyn Error>>
{
    use solarity_cpu::{
        CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass, CpuStoragePlan, JobOutcome,
        LoadBatch,
    };
    use std::num::NonZeroUsize;
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe()?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let dependency = request.dependency(cpu.storage(), CpuStorageClass::Required)?;
    let mut jobs = vec![71];
    let mut load = LoadBatch::new(CpuService::Required, |value: &mut usize| {
        *value = 0;
        JobOutcome::Succeeded
    });
    load.start_after(&cpu, &mut jobs, &[dependency.readiness()])?;
    drop(request);
    drop(producer);
    assert!(load.reclaim(&mut jobs).is_err());
    assert_eq!(jobs, [71]);
    assert!(matches!(
        dependency.poll(),
        Some(Err(M2LoadError::Abandoned))
    ));
    cpu.shutdown()?;
    Ok(())
}

/// Registration racing terminal publication must neither miss nor double-complete an edge.
#[test]
fn model_dependency_registration_races_abandonment_without_lost_readiness()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let budget = solarity_cpu::CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(
        64 << 20,
        64 << 20,
        16 << 20,
    ));
    for _ in 0..64 {
        let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
            return Err("missing producer".into());
        };
        let request = producer.subscribe()?;
        let ready = std::sync::Barrier::new(2);
        let dependency = std::thread::scope(|scope| {
            let registration = scope.spawn(|| {
                ready.wait();
                request.dependency(&budget, solarity_cpu::CpuStorageClass::Required)
            });
            ready.wait();
            drop(producer);
            registration.join()
        })
        .map_err(|_| "registration panicked")??;
        assert_eq!(
            dependency.readiness().outcome()?,
            Some(solarity_cpu::JobOutcome::Failed)
        );
    }
    Ok(())
}

#[path = "model_storage_requests.rs"]
mod storage;
