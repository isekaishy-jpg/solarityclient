//! Parsed texture ownership survives cache transfer, pressure and worker snapshots.
use super::texture::{dxt_blp_mips, raw3_blp, raw3_blp_mips};
use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetReadBudget, AssetStore, BlpTextureCache,
    BlpTextureSource, ClientDataRoot, Locale,
};
use solarity_cpu::{
    CpuService, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan,
};
use std::{error::Error, sync::Arc};

fn catalog(fixture: &Fixture) -> Result<ArchiveCatalog, Box<dyn Error>> {
    Ok(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)
}

#[test]
fn authored_mip_capacity_survives_direct_clones_and_store_teardown() -> Result<(), Box<dyn Error>> {
    let raw = raw3_blp_mips(&[(2, 2, &[0xff123456; 4]), (1, 1, &[0xffabcdef])]);
    let bc1 = dxt_blp_mips(2, 0, 0, &[(4, 4, &[0; 8]), (2, 2, &[0; 8])]);
    let bc2 = dxt_blp_mips(2, 1, 1, &[(4, 4, &[0; 16])]);
    let bc3 = dxt_blp_mips(2, 8, 7, &[(4, 4, &[0; 16])]);
    let mut indexed = raw3_blp(1, 1, &[0]);
    indexed[8] = 1;
    indexed[10] = 0;
    for bytes in [raw, bc1, bc2, bc3, indexed] {
        let fixture = Fixture::new(&[FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Test.blp",
            bytes: &bytes,
        }])?;
        let catalog = catalog(&fixture)?;
        let namespace = catalog.namespace();
        let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 0));
        catalog
            .model_cache_service()
            .configure_storage(budget.clone())?;
        let mut store = AssetStore::mount(catalog)?;
        let source = BlpTextureSource::load(&mut store, &AssetPath::new("Textures/Test.blp")?)?;
        let charged = source.resident_bytes();
        assert!(charged > 16, "source metadata and payload remain owned");
        assert_eq!(
            budget.snapshot().used(Class::Required),
            charged,
            "encoded input has retired"
        );
        let copy = source.clone();
        assert_eq!(copy.namespace(), namespace);
        let decoded = source.decode_mip(0)?;
        drop((source, store));
        assert_eq!(budget.snapshot().used(Class::Required), charged);
        assert_eq!(copy.decode_mip(0)?.rgba8(), decoded.rgba8());
        drop(copy);
        assert_eq!(budget.snapshot().used(Class::Required), 0);
    }
    Ok(())
}

#[test]
fn merged_speculative_texture_promotes_once_and_retries_after_pressure()
-> Result<(), Box<dyn Error>> {
    let bytes = raw3_blp(1, 1, &[0xff123456]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/Test.blp",
        bytes: &bytes,
    }])?;
    let catalog = catalog(&fixture)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 1 << 20));
    catalog
        .model_cache_service()
        .configure_storage(budget.clone())?;
    let mut main = AssetStore::mount(catalog.clone())?;
    let mut worker = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Textures/Test.blp")?;
    let policy = AssetReadBudget::for_service(budget.clone(), CpuService::Speculative);
    let mut prepared = BlpTextureCache::new();
    let source = worker.with_read_budget(&policy, |store| prepared.load(store, &path))?;
    let charged = source.resident_bytes();
    assert_eq!(budget.snapshot().used(Class::Speculative), charged);
    assert_eq!(budget.snapshot().bytes(Class::Required, Kind::Result), 0);
    let mut cache = BlpTextureCache::new();
    assert_eq!(cache.merge(prepared), 1);
    let blocker = budget.reserve(
        Class::Required,
        Kind::Result,
        (1 << 20) - budget.snapshot().used(Class::Required),
    )?;
    assert!(matches!(
        cache.load(&mut main, &path),
        Err(AssetError::ReadAdmission { .. })
    ));
    assert_eq!(budget.snapshot().used(Class::Speculative), charged);
    assert_eq!(cache.len(), 1);
    drop(blocker);
    source.admit_required(&budget)?;
    let required = cache.load(&mut main, &path)?;
    assert!(Arc::ptr_eq(&required, &source));
    assert_eq!(budget.snapshot().used(Class::Speculative), 0);
    assert_eq!(
        budget.snapshot().bytes(Class::Required, Kind::Result),
        charged
    );
    let optional = worker.with_read_budget(&policy, |store| cache.load(store, &path))?;
    assert!(Arc::ptr_eq(&optional, &source));
    assert_eq!(
        budget.snapshot().bytes(Class::Required, Kind::Result),
        charged
    );
    let snapshot = (*source).clone();
    drop((source, required, optional, worker, main));
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(
        budget.snapshot().bytes(Class::Required, Kind::Result),
        charged,
        "upload snapshot pins the inner owner"
    );
    drop(snapshot);
    assert!(budget.snapshot().bytes(Class::Required, Kind::Metadata) > 0);
    cache.collect_unused();
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    assert_eq!(budget.snapshot().bytes(Class::Required, Kind::Result), 0);
    Ok(())
}

#[test]
fn texture_admission_and_parse_failure_release_inputs_without_caching_failure()
-> Result<(), Box<dyn Error>> {
    let bytes = raw3_blp(1, 1, &[0xff123456]);
    let malformed = &bytes[..bytes.len() - 1];
    let oversized = raw3_blp(4096, 4096, &[0]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Test.blp",
            bytes: &bytes,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Bad.blp",
            bytes: malformed,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Oversized.blp",
            bytes: &oversized,
        },
    ])?;
    let catalog = catalog(&fixture)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 0));
    catalog
        .model_cache_service()
        .configure_storage(budget.clone())?;
    let mut store = AssetStore::mount(catalog)?;
    let mut cache = BlpTextureCache::new();
    let path = AssetPath::new("Textures/Test.blp")?;
    // The archive input fits; the separate parsed-payload reservation cannot.
    let blocked_bytes = (1 << 20) - bytes.len() - 16;
    let blocker = budget.reserve(Class::Required, Kind::Result, blocked_bytes)?;
    assert!(matches!(
        cache.load(&mut store, &path),
        Err(AssetError::ReadAdmission { .. })
    ));
    assert!(cache.is_empty());
    assert_eq!(
        budget.snapshot().bytes(Class::Required, Kind::Result),
        blocked_bytes
    );
    drop(blocker);
    assert!(matches!(
        cache.load(&mut store, &AssetPath::new("Textures/Bad.blp")?),
        Err(AssetError::TextureDecode { .. })
    ));
    assert_eq!(budget.snapshot().bytes(Class::Required, Kind::Result), 0);
    assert!(matches!(
        cache.load(&mut store, &AssetPath::new("Textures/Oversized.blp")?),
        Err(AssetError::ReadAdmission { .. })
    ));
    assert_eq!(budget.snapshot().bytes(Class::Required, Kind::Result), 0);
    let source = cache.load(&mut store, &path)?;
    assert_eq!(source.decode_mip(0)?.rgba8(), &[0x12, 0x34, 0x56, 0xff]);
    drop((source, cache));
    assert!(budget.snapshot().bytes(Class::Required, Kind::Metadata) > 0);
    drop(store);
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    assert_eq!(budget.snapshot().bytes(Class::Required, Kind::Result), 0);
    Ok(())
}

#[test]
fn cached_offline_source_adopts_namespace_admission_without_reparsing() -> Result<(), Box<dyn Error>>
{
    let bytes = raw3_blp(1, 1, &[0xff123456]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/Test.blp",
        bytes: &bytes,
    }])?;
    let mut store = AssetStore::mount(catalog(&fixture)?)?;
    let path = AssetPath::new("Textures/Test.blp")?;
    let mut cache = BlpTextureCache::new();
    let offline = cache.load(&mut store, &path)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 1 << 20, 0));
    store
        .model_cache_service()
        .configure_storage(budget.clone())?;
    let admitted = cache.load(&mut store, &path)?;
    assert!(Arc::ptr_eq(&offline, &admitted));
    assert_eq!(
        budget.snapshot().used(Class::Required),
        offline.resident_bytes()
    );
    drop((offline, admitted, cache, store));
    assert_eq!(budget.snapshot().used(Class::Required), 0);
    Ok(())
}
