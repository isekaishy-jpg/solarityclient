//! Source payload charges survive sharing and move only at required consumption.
use super::*;
use solarity_asset::AssetError;
use solarity_cpu::{
    CpuError, CpuService, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStoragePlan,
};

/// Encoded read ownership and a decoded shared generation must retire independently.
#[test]
fn encoded_and_decoded_source_charges_compose_across_publication() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 1 << 20));
    sources.configure_storage(budget.clone())?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let mut store = AssetStore::mount(catalog)?;
    let required =
        solarity_asset::AssetReadBudget::for_service(budget.clone(), CpuService::Required);
    let encoded = store
        .with_read_budget(&required, |store| store.read(key.path()))?
        .into_bytes();
    let encoded_charge = budget.snapshot().used(Class::Required);
    assert!(encoded_charge >= encoded.len());
    let M2Load::Producer(producer) = sources.request_for(&key, CpuService::Speculative)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe_for(CpuService::Speculative);
    let speculative =
        solarity_asset::AssetReadBudget::for_service(budget.clone(), CpuService::Speculative);
    let model = producer.load_admitted(&mut store, &speculative)?;
    let decoded_charge = model.resident_storage_bytes();
    assert_eq!(budget.snapshot().used(Class::Required), encoded_charge);
    assert_eq!(budget.snapshot().used(Class::Speculative), decoded_charge);
    drop(encoded);
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    request.set_service(CpuService::Required);
    let delivered = request.poll().ok_or("missing published source")??;
    assert!(ResourceLease::ptr_eq(&model, &delivered));
    assert_eq!(budget.snapshot().used(Class::Speculative), 0);
    assert_eq!(budget.snapshot().used(Class::Required), decoded_charge);
    drop((request, model, delivered, store, sources));
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}

#[test]
fn speculative_model_charge_promotes_once_and_refused_consumption_can_retry()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 1 << 20));
    sources.configure_storage(budget.clone())?;
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = sources.request_for(&key, CpuService::Speculative)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe_for(CpuService::Speculative);
    let edges = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 0, 0));
    let dependency = request.dependency(&edges, Class::Frame)?;
    let mut store = AssetStore::mount(catalog)?;
    let model = producer.load(&mut store)?;
    let bytes = model.resident_storage_bytes();
    assert_eq!(
        budget.snapshot().bytes(Class::Speculative, Kind::Result),
        bytes
    );
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    let blocker = budget.reserve(Class::Required, Kind::Result, 1 << 20)?;
    request.set_service(CpuService::Required);
    assert!(
        matches!(dependency.poll(), Some(Err(M2LoadError::Asset(error))) if matches!(&*error, AssetError::SourceStorage(CpuError::StorageAtCapacity { class: Class::Required, .. })))
    );
    assert_eq!(budget.snapshot().used(Class::Speculative), bytes);
    drop(blocker);
    let delivered = dependency.poll().ok_or("missing result")??;
    assert!(ResourceLease::ptr_eq(&model, &delivered));
    assert_eq!(budget.snapshot().used(Class::Speculative), 0);
    assert_eq!(budget.snapshot().used(Class::Required), bytes);
    let M2Load::Ready(reused) = sources.request(&key)? else {
        return Err("generation was not retained".into());
    };
    assert!(ResourceLease::ptr_eq(&model, &reused));
    assert_eq!(budget.snapshot().used(Class::Required), bytes);
    drop((model, delivered, reused, request, dependency));
    assert_eq!(
        budget.snapshot().used(Class::Required),
        bytes,
        "qualified cache-only retention stays charged"
    );
    drop(store);
    drop(sources);
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}

#[test]
fn refused_model_payload_is_shared_failure_without_poisoning_source_identity()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = catalog.model_cache_service();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    sources.configure_storage(budget.clone())?;
    assert!(matches!(
        sources.configure_storage(budget.clone()),
        Err(AssetError::SourceStorageConfigured)
    ));
    let key = AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("Creature/Solarity/Shared.m2")?,
    );
    let M2Load::Producer(producer) = sources.request(&key)? else {
        return Err("missing producer".into());
    };
    let request = producer.subscribe();
    let mut store = AssetStore::mount(catalog)?;
    let Err(M2LoadError::Asset(original)) = producer.load(&mut store) else {
        return Err("payload bypassed zero budget".into());
    };
    let Some(Err(M2LoadError::Asset(delivered))) = request.poll() else {
        return Err("missing shared failure".into());
    };
    assert!(Arc::ptr_eq(&original, &delivered));
    assert!(matches!(
        &*original,
        AssetError::ReadAdmission {
            source: CpuError::StorageAtCapacity {
                class: Class::Required,
                ..
            },
            ..
        }
    ));
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    assert!(
        matches!(sources.request(&key)?, M2Load::Producer(_)),
        "refusal installs no successful source or negative-cache entry"
    );
    Ok(())
}
