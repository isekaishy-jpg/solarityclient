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
    let cancelled = producer.subscribe();
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
    let joined = producer.subscribe();
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
    let joined = producer.subscribe();
    let worker_consumer = joined.clone();
    let mut cpu = solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        std::num::NonZeroUsize::MIN,
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
    let joined = producer.subscribe();
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
    let prewarm = producer.subscribe_for(solarity_cpu::CpuService::Speculative);
    let mut store = AssetStore::mount(catalog)?;
    let mut cpu = solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        std::num::NonZeroUsize::MIN,
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
