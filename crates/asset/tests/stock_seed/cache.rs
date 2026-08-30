//! External stock-compatibility tests for decoded asset cache lifetime.

use std::error::Error;
use std::sync::Arc;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, M2ModelCache};

use crate::model::{m2_bytes, skin_bytes};
use crate::support::{Fixture, FixtureFile};

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
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Cached.m2")?;
    let mut cache = M2ModelCache::new();

    let first = cache.load(&mut store, &path)?;
    let second = cache.load(&mut store, &path)?;

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.name(), Some("HdModel"));
    assert_eq!(first.skins()[0].bone_count_max(), 512);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.collect_unused(), 0);

    let weak = Arc::downgrade(&first);
    drop(first);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    assert!(weak.upgrade().is_none());
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
