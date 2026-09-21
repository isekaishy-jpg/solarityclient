//! Real archive reads retain admission through transfer and fail before domain decode.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetReadBudget, AssetResourceKey, AssetStore,
    ClientDataRoot, Locale, M2Load, M2LoadError, WdbcTable,
};
use solarity_cpu::{CpuService, CpuStorageBudget, CpuStorageClass, CpuStoragePlan};
use std::{
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?)
}

#[test]
fn source_charge_survives_store_and_shared_byte_ownership() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Test/source.bin",
        bytes: b"encoded payload",
    }])?;
    let mut store = mount(&fixture)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 4096, 0));
    let policy = AssetReadBudget::for_service(budget.clone(), CpuService::Required);
    let path = AssetPath::new("Test/source.bin")?;
    let bytes = store
        .with_read_budget(&policy, |store| store.read(&path))?
        .into_bytes();
    let charged = bytes.admitted_bytes();
    assert!(charged >= bytes.len());
    let owner = Arc::new(bytes);
    let consumer = Arc::clone(&owner);
    drop((store, owner));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), charged);
    assert_eq!(consumer.as_slice(), b"encoded payload");
    drop(consumer);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn admission_failure_precedes_decode_and_is_published_to_joiners() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Test/model.m2",
        bytes: b"malformed model",
    }])?;
    let mut store = mount(&fixture)?;
    let path = AssetPath::new("Test/model.m2")?;
    let key = AssetResourceKey::new(store.namespace(), path);
    let M2Load::Producer(producer) = store.model_cache_service().request(&key)? else {
        return Err("missing producer".into());
    };
    let joined = producer.subscribe();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 4096, 0));
    let policy = AssetReadBudget::for_service(budget.clone(), CpuService::Speculative);
    let error = producer
        .load_admitted(&mut store, &policy)
        .err()
        .ok_or("unexpected decode")?;
    let delivered = joined
        .poll()
        .ok_or("unpublished failure")?
        .err()
        .ok_or("unexpected joined success")?;
    let (M2LoadError::Asset(error), M2LoadError::Asset(delivered)) = (error, delivered) else {
        return Err("wrong shared error".into());
    };
    assert!(matches!(&*error, AssetError::ReadAdmission { .. }));
    assert!(Arc::ptr_eq(&error, &delivered));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Speculative), 0);
    Ok(())
}

#[test]
fn nested_policy_restores_after_refusal_and_unwind_without_changing_lookup()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Test/source.bin",
        bytes: b"source",
    }])?;
    let mut store = mount(&fixture)?;
    let path = AssetPath::new("Test/source.bin")?;
    let absent = AssetPath::new("Test/absent.bin")?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 4096, 0));
    let required = AssetReadBudget::for_service(budget.clone(), CpuService::Required);
    let speculative = AssetReadBudget::for_service(budget.clone(), CpuService::Speculative);
    store.with_read_budget(&required, |store| -> Result<(), Box<dyn Error>> {
        assert!(matches!(
            store.with_read_budget(&speculative, |store| store.read(&path)),
            Err(AssetError::ReadAdmission { .. })
        ));
        assert!(matches!(
            store.with_read_budget(&speculative, |store| store.read(&absent)),
            Err(AssetError::AssetNotFound { .. })
        ));
        assert!(
            catch_unwind(AssertUnwindSafe(|| store
                .with_read_budget(&speculative, |_| panic!(
                    "controlled decoder unwind"
                ))))
            .is_err()
        );
        let bytes = store.read(&path)?.into_bytes();
        assert!(bytes.admitted_bytes() > 0);
        Ok(())
    })?;
    assert_eq!(store.read(&path)?.into_bytes().admitted_bytes(), 0);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn retained_database_and_loose_addon_bytes_keep_their_reservations() -> Result<(), Box<dyn Error>> {
    let mut table = b"WDBC".to_vec();
    for value in [0u32, 1, 4, 1] {
        table.extend_from_slice(&value.to_le_bytes());
    }
    table.push(0);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient/test.dbc",
        bytes: &table,
    }])?;
    fixture.write_loose_file("Interface/AddOns/Test/source.lua", b"return 7")?;
    let mut store = mount(&fixture)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 4096, 0));
    let policy = AssetReadBudget::for_service(budget.clone(), CpuService::Required);
    let table_path = AssetPath::new("DBFilesClient/test.dbc")?;
    let addon_path = AssetPath::new("Interface/AddOns/Test/source.lua")?;
    let table = store.with_read_budget(&policy, |store| WdbcTable::load(store, &table_path))?;
    let table_charge = budget.snapshot().used(CpuStorageClass::Required);
    assert!(table_charge >= 21);
    let addon = store.with_read_budget(&policy, |store| store.read_addon_file(&addon_path))?;
    assert_eq!(
        budget.snapshot().used(CpuStorageClass::Required),
        table_charge + addon.admitted_bytes()
    );
    drop(store);
    assert_eq!(addon, b"return 7");
    drop(table);
    assert_eq!(
        budget.snapshot().used(CpuStorageClass::Required),
        addon.admitted_bytes()
    );
    drop(addon);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

/// Cloned UI handles share admission without retaining a mutable reader borrow.
#[test]
fn handle_scope_and_utf8_source_keep_exact_allocation_ownership() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Test/script.lua",
            bytes: b"return 42",
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Test/invalid.lua",
            bytes: &[255, 254],
        },
    ])?;
    let handle = solarity_asset::AssetStoreHandle::new(mount(&fixture)?);
    let consumer = handle.clone();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 4096, 0));
    let policy = AssetReadBudget::for_service(budget.clone(), CpuService::Required);
    let path = AssetPath::new("Test/script.lua")?;
    let source = handle
        .with_read_budget(&policy, || consumer.borrow_mut().read(&path))?
        .into_bytes();
    let address = source.as_ptr();
    let charged = source.admitted_bytes();
    let source = Arc::new(source.into_text()?);
    assert_eq!(source.as_ptr(), address);
    assert_eq!(source.as_str(), "return 42");
    let copy = Arc::clone(&source);
    drop(source);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), charged);
    let invalid = AssetPath::new("Test/invalid.lua")?;
    let bytes = handle
        .with_read_budget(&policy, || consumer.borrow_mut().read(&invalid))?
        .into_bytes();
    assert!(bytes.into_text().is_err());
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), charged);
    assert!(
        catch_unwind(AssertUnwindSafe(|| handle.with_read_budget(
            &policy,
            || {
                let _borrow = consumer.borrow_mut();
                panic!("controlled handle consumer unwind");
            }
        )))
        .is_err()
    );
    assert_eq!(
        consumer
            .borrow_mut()
            .read(&path)?
            .into_bytes()
            .admitted_bytes(),
        0
    );
    drop((handle, consumer, copy));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
