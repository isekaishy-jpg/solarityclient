//! Real allocation bounds, rollback, and pressure-driven font cache lifetime.
use super::*;
use solarity_cpu::{CpuService, CpuStorageBudget, CpuStorageClass, CpuStoragePlan};
use std::error::Error;

fn policy(limit: usize) -> AssetReadBudget {
    AssetReadBudget::for_service(
        CpuStorageBudget::new(CpuStoragePlan::new(0, limit, 0)),
        CpuService::Required,
    )
}

#[test]
fn cache_table_bound_covers_pinned_hashbrown_layouts() -> Result<(), Box<dyn Error>> {
    #[repr(align(64))]
    #[derive(Eq, Hash, PartialEq)]
    struct Aligned(u8);
    fn check<K: Eq + Hash, V>() -> Result<(), Box<dyn Error>> {
        for capacity in 1..=256 {
            let mut map: Map<K, V> = Map::with_hasher(RandomState::new());
            map.try_reserve(capacity)?;
            assert!(table_bound::<(K, V)>(capacity)? >= map.allocation_size());
        }
        Ok(())
    }
    check::<(), ()>()?;
    check::<u8, u8>()?;
    check::<u64, Vec<u8>>()?;
    check::<Aligned, Aligned>()?;
    Ok(())
}

#[test]
fn admitted_tables_and_buffers_preserve_old_storage_when_growth_is_denied()
-> Result<(), Box<dyn Error>> {
    let policy = policy(4096);
    let budget = policy.storage();
    let mut map = CacheMap::default();
    map.insert(Some(&policy), 0_u64, 11_u64)?;
    let capacity = map.capacity();
    for key in 1..capacity {
        map.insert(Some(&policy), key as u64, key as u64)?;
    }
    let allocated = map.allocation_size();
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), allocated);
    let pointer = map.get(&0).ok_or("map entry")? as *const u64;
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        4096 - allocated,
    )?;
    assert!(map.insert(Some(&policy), capacity as u64, 77).is_err());
    assert_eq!(map.get(&0).ok_or("map entry")? as *const u64, pointer);
    assert_eq!(map.get(&0), Some(&11));
    map.insert(Some(&policy), 0, 13)?;
    assert_eq!(map.get(&0), Some(&13));
    drop(hold);
    map.insert(Some(&policy), capacity as u64, 77)?;
    assert!(budget.snapshot().peak(CpuStorageClass::Required) >= allocated + map.allocation_size());
    drop(map);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    let mut bytes = FontBuffer::zeroed(Some(&policy), 2048)?;
    bytes[0] = 42;
    let pointer = bytes.as_ptr();
    assert!(bytes.reserve(Some(&policy), 4096).is_err());
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes[0], 42);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 2048);
    drop(bytes);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn font_pressure_reclaims_only_unpinned_coverage_before_retrying_exact_request()
-> Result<(), Box<dyn Error>> {
    use crate::test_support::{Fixture, FixtureFile};
    use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
    let fixture = Fixture::new(&[FixtureFile {
        path: "Fonts/Test.ttf",
        bytes: include_bytes!("../fixtures/tooltip_fixture.ttf"),
    }])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let policy = policy(64 << 20);
    let mut fonts = crate::FontSystem::new()?;
    let path = AssetPath::new("Fonts/Test.ttf")?;
    let mode = crate::FontRasterization::Antialiased;
    let pinned = assets.with_read_budget(&policy, |assets| -> Result<_, FontError> {
        let pinned = fonts.rasterize(assets, &path, 16, 'A', mode)?;
        drop(fonts.rasterize(assets, &path, 128, 'A', mode)?);
        fonts.measure_line_width_26_6(assets, &path, 16, "AAA", mode)?;
        Ok(pinned)
    })?;
    let before = policy.storage().snapshot().used(CpuStorageClass::Required);
    let hold = policy.storage().reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        (64 << 20) - before,
    )?;
    let next = assets.with_read_budget(&policy, |assets| {
        fonts.rasterize(assets, &path, 20, 'A', mode)
    })?;
    assert_eq!(fonts.cached_glyph_count(), 2);
    let cached = assets.with_read_budget(&policy, |assets| {
        fonts.rasterize(assets, &path, 16, 'A', mode)
    })?;
    assert_eq!(cached.coverage().as_ptr(), pinned.coverage().as_ptr());
    let mut serial = crate::FontSystem::new()?;
    assert_eq!(next, serial.rasterize(&mut assets, &path, 20, 'A', mode)?);
    drop(hold);
    drop(cached);
    drop(next);
    drop(pinned);
    fonts.trim_unused(&mut assets)?;
    assert_eq!(
        policy.storage().snapshot().used(CpuStorageClass::Required),
        0
    );
    Ok(())
}
