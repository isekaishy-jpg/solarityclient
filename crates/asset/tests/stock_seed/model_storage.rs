//! Decoded accounting follows expanded key buffers, not encoded input file size.
use super::*;

#[test]
fn decoded_storage_counts_nested_keys_and_releases_uncached_generations()
-> Result<(), Box<dyn Error>> {
    let small = animated_m2_bytes()?;
    let mut large = small.clone();
    let word = |bytes: &[u8], offset: usize| -> Result<usize, Box<dyn Error>> {
        Ok(u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize)
    };
    let bone = word(&large, 0x30)?;
    let times = word(&large, bone + 24)?;
    let values = word(&large, bone + 32)?;
    let start = large.len() as u32;
    for tick in 0..258_u32 {
        large.extend_from_slice(&tick.to_le_bytes());
    }
    large[times..times + 4].copy_from_slice(&258_u32.to_le_bytes());
    large[times + 4..times + 8].copy_from_slice(&start.to_le_bytes());
    let start = large.len() as u32;
    for _ in 0..258 {
        for value in [1_f32, 2., 3.] {
            large.extend_from_slice(&value.to_le_bytes());
        }
    }
    large[values..values + 4].copy_from_slice(&258_u32.to_le_bytes());
    large[values + 4..values + 8].copy_from_slice(&start.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Small.m2",
            bytes: &small,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Small00.skin",
            bytes: &skin,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Large.m2",
            bytes: &large,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Large00.skin",
            bytes: &skin,
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
    let small = DecodedM2Model::load(&mut store, &AssetPath::new("Small.m2")?)?;
    let large = DecodedM2Model::load(&mut store, &AssetPath::new("Large.m2")?)?;
    assert_eq!(
        large.animations().bones()[0].translation().channels()[0]
            .values()
            .len(),
        258
    );
    assert_eq!(
        large.resident_storage_bytes() - small.resident_storage_bytes(),
        256 * (size_of::<u32>() + size_of::<glam::Vec3>())
    );
    let class = solarity_cpu::CpuStorageClass::Required;
    let kind = solarity_cpu::CpuStorageKind::Result;
    assert_eq!(
        budget.snapshot().bytes(class, kind),
        small.resident_storage_bytes() + large.resident_storage_bytes()
    );
    let retained = large.resident_storage_bytes();
    drop(small);
    assert_eq!(budget.snapshot().bytes(class, kind), retained);
    drop(large);
    assert_eq!(budget.snapshot().bytes(class, kind), 0);
    assert!(
        budget.snapshot().used(class) > 0,
        "the mounted namespace retains its controls"
    );
    drop(store);
    assert_eq!(budget.snapshot().used(class), 0);
    Ok(())
}
