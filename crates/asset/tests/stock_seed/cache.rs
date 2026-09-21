//! External stock-compatibility tests for decoded asset cache lifetime.

use solarity_asset::ResourceLease;
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureCache, ClientDataRoot, DecodedM2Model, Locale,
    M2ModelCache, ResourceCacheClock,
};

use crate::model::{m2_bytes, skin_bytes};
use crate::support::{Fixture, FixtureFile};
use crate::texture::raw3_blp;

/// Repeated scene requests share one HD-sized decode until explicit collection.
#[test]
fn m2_cache_shares_path_decode_and_collects_unreferenced_models() -> Result<(), Box<dyn Error>> {
    let stock_model = m2_bytes("StockModel", 1)?;
    let hd_model = m2_bytes("HdModel", 1)?;
    let stock_skin = skin_bytes(32, &[0, 1, 2])?;
    let hd_skin = skin_bytes(512, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Cached.m2",
            bytes: &stock_model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Cached00.skin",
            bytes: &stock_skin,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Cached.m2",
            bytes: &hd_model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Cached00.skin",
            bytes: &hd_skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let budget =
        solarity_cpu::CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(0, 1 << 20, 0));
    catalog
        .model_cache_service()
        .configure_storage(budget.clone())?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Cached.m2")?;
    let clock = Arc::new(AtomicU32::new(0));
    let mut cache =
        M2ModelCache::with_clock(ResourceCacheClock::from_milliseconds(Arc::clone(&clock)));

    let first = cache.load(&mut store, &path)?;
    let second = cache.load(&mut store, &path)?;

    let charged = first.resident_storage_bytes();
    assert_eq!(
        budget.snapshot().bytes(
            solarity_cpu::CpuStorageClass::Required,
            solarity_cpu::CpuStorageKind::Result
        ),
        charged
    );
    assert!(ResourceLease::ptr_eq(&first, &second));
    assert_eq!(first.name(), Some("HdModel"));
    assert_eq!(first.skins()[0].bone_count_max(), 512);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.collect_unused(), 0);

    let weak = ResourceLease::downgrade(&first);
    drop(first);
    drop(second);
    clock.store(9_999, Ordering::Release);
    assert_eq!(cache.collect_unused(), 0);
    assert!(weak.is_alive());
    assert_eq!(
        budget.snapshot().bytes(
            solarity_cpu::CpuStorageClass::Required,
            solarity_cpu::CpuStorageKind::Result
        ),
        charged
    );
    clock.store(10_000, Ordering::Release);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    assert!(weak.upgrade().is_none());
    assert_eq!(
        budget.snapshot().bytes(
            solarity_cpu::CpuStorageClass::Required,
            solarity_cpu::CpuStorageKind::Result
        ),
        0
    );
    // Warm control storage survives payload collection; weak pin storage outlives the cache.
    assert!(
        budget.snapshot().bytes(
            solarity_cpu::CpuStorageClass::Required,
            solarity_cpu::CpuStorageKind::Metadata
        ) > 0
    );
    drop((cache, store));
    assert!(
        budget
            .snapshot()
            .used(solarity_cpu::CpuStorageClass::Required)
            > 0
    );
    drop(weak);
    assert_eq!(
        budget
            .snapshot()
            .used(solarity_cpu::CpuStorageClass::Required),
        0
    );
    Ok(())
}

/// Runtime cache reads only the one SKIN selected by the Vulkan capability tier.
#[test]
fn m2_cache_does_not_read_unselected_hd_skin_profiles() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("PrimaryOnly", 2)?;
    let skin = skin_bytes(96, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\PrimaryOnly.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\PrimaryOnly00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\PrimaryOnly.m2")?;

    // Exhaustive tooling still proves that the header's second companion is absent.
    assert!(DecodedM2Model::load(&mut store, &path).is_err());
    let mut cache = M2ModelCache::new();
    let selected = cache.load(&mut store, &path)?;
    assert_eq!(selected.skins().len(), 1);
    assert_eq!(selected.skin_profile_count(), 2);
    assert_eq!(selected.skins()[0].bone_count_max(), 96);
    Ok(())
}

/// A failed decode does not poison later cache ownership or hide its error.
#[test]
fn m2_cache_does_not_retain_failed_loads() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("MissingSkin", 1)?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Creature\\Solarity\\Missing.m2",
        bytes: &model,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Missing.m2")?;
    let mut cache = M2ModelCache::new();

    assert!(cache.load(&mut store, &path).is_err());
    assert!(cache.is_empty());
    assert!(cache.load(&mut store, &path).is_err());
    assert!(cache.is_empty());
    Ok(())
}

/// Texture cache identity remains the logical path when an HD patch wins.
#[test]
fn blp_cache_shares_selected_hd_source_and_collects_it() -> Result<(), Box<dyn Error>> {
    let stock = raw3_blp(1, 1, &[0xFFFF_0000]);
    let hd = raw3_blp(2, 1, &[0xFF00_FF00, 0xFF00_FF00]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Character\\Solarity\\Cached.blp",
            bytes: &stock,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Character\\Solarity\\Cached.blp",
            bytes: &hd,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Character/Solarity/Cached.blp")?;
    let mut cache = BlpTextureCache::new();

    let first = cache.load(&mut store, &path)?;
    let second = cache.load(&mut store, &path)?;

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!((first.width(), first.height()), (2, 1));
    assert_eq!(
        first.source().relative_path().to_string_lossy(),
        "patch-A.MPQ"
    );
    assert_eq!(cache.collect_unused(), 0);

    drop(first);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    Ok(())
}

/// Mount handles share only an explicitly cloned immutable plan; rediscovery is new content authority.
#[test]
fn model_cache_qualifies_aliases_by_namespace_and_reuses_shared_mount_plans()
-> Result<(), Box<dyn Error>> {
    let alpha = m2_bytes("Alpha", 1)?;
    let beta = m2_bytes("Beta", 1)?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let first = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Test.m2",
            bytes: &alpha,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Test00.skin",
            bytes: &skin,
        },
    ])?;
    let second = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Test.m2",
            bytes: &beta,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature/Test00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(first.data_root())?, Locale::EnUs)?;
    let mut a = AssetStore::mount(catalog.clone())?;
    let mut alias = AssetStore::mount(catalog)?;
    let mut rediscovered = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(first.data_root())?,
        Locale::EnUs,
    )?)?;
    let mut b = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(second.data_root())?,
        Locale::EnUs,
    )?)?;
    assert_ne!(a.identity(), alias.identity());
    assert_eq!(a.namespace(), alias.namespace());
    assert_ne!(a.namespace(), rediscovered.namespace());
    let path = AssetPath::new("Creature/Test.m2")?;
    let mut cache = M2ModelCache::new();
    let one = cache.load(&mut a, &path)?;
    let shared = cache.load(&mut alias, &AssetPath::new("creature/test.mdx")?)?;
    assert!(ResourceLease::ptr_eq(&one, &shared));
    let other = cache.load(&mut b, &path)?;
    assert_eq!(other.name(), Some("Beta"));
    assert_eq!(one.name(), Some("Alpha"));
    assert!(!ResourceLease::ptr_eq(&one, &other));
    let new_generation = cache.load(&mut rediscovered, &path)?;
    assert!(!ResourceLease::ptr_eq(&one, &new_generation));
    let missing = Fixture::new(&[])?;
    let mut missing = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(missing.data_root())?,
        Locale::EnUs,
    )?)?;
    assert!(matches!(
        cache.load(&mut missing, &path),
        Err(solarity_asset::AssetError::AssetNotFound { .. })
    ));
    assert_eq!(cache.len(), 3);
    Ok(())
}

/// Merging optional worker results cannot overwrite the same path in another stack.
#[test]
fn texture_merge_and_upload_enumeration_preserve_namespace() -> Result<(), Box<dyn Error>> {
    let first = raw3_blp(1, 1, &[0xFFFF_0000]);
    let second = raw3_blp(2, 1, &[0xFF00_FF00, 0xFF00_FF00]);
    let a = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/Test.blp",
        bytes: &first,
    }])?;
    let b = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/Test.blp",
        bytes: &second,
    }])?;
    let mut a = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(a.data_root())?,
        Locale::EnUs,
    )?)?;
    let mut b = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(b.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Textures/Test.blp")?;
    let mut cache = BlpTextureCache::new();
    let mut worker = BlpTextureCache::new();
    let first = cache.load(&mut a, &path)?;
    let second = worker.load(&mut b, &path)?;
    assert_eq!(cache.merge(worker), 1);
    assert_eq!(cache.len(), 2);
    assert!(Arc::ptr_eq(&cache.load(&mut a, &path)?, &first));
    assert!(Arc::ptr_eq(&cache.load(&mut b, &path)?, &second));
    assert_eq!(
        cache
            .entries(a.namespace())
            .map(|(_, source)| source.width())
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        cache
            .entries(b.namespace())
            .map(|(_, source)| source.width())
            .collect::<Vec<_>>(),
        [2]
    );
    drop(first);
    assert_eq!(cache.collect_unused(), 1);
    assert_eq!(cache.entries(b.namespace()).count(), 1);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    Ok(())
}
