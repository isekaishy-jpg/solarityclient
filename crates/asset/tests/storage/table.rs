use super::*;
use std::error::Error;

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
fn connected_table_admission_preserves_values_and_reuses_backing() -> Result<(), Box<dyn Error>> {
    use solarity_cpu::{
        CpuBuffer, CpuStorageBudget, CpuStorageClass as Class, CpuStoragePlan, CpuStorageWorkingSet,
    };
    let source = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 0, 0));
    let policy = AssetReadBudget::for_class(source.clone(), Class::Frame);
    let mut table = AssetStorageMap::<u64, u64>::metadata();
    table.insert(Some(&policy), 5, 17)?;
    let address = std::ptr::from_ref(table.get(&5).ok_or("retained entry")?);
    let allocation = table
        .memory
        .as_ref()
        .ok_or("table admission")?
        .allocation_id();
    let used = source.snapshot().used(Class::Frame);
    // Adoption refusal cannot detach the table's original backing or charge.
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(used - 1, 0, 0));
    assert!(
        refused
            .reserve_working_set(
                Class::Frame,
                table.reservation_bytes(
                    &AssetReadBudget::for_class(refused.clone(), Class::Frame),
                    1
                )?
            )
            .is_err()
    );
    assert_eq!(source.snapshot().used(Class::Frame), used);
    assert_eq!(
        std::ptr::from_ref(table.get(&5).ok_or("entry after refusal")?),
        address
    );
    let destination = CpuStorageBudget::new(CpuStoragePlan::new(used, 0, 0));
    let mut adoption = destination.reserve_working_set(Class::Frame, used)?;
    table.reserve_reserved(&mut adoption, 1)?;
    assert_eq!(
        table
            .memory
            .as_ref()
            .ok_or("adopted table")?
            .allocation_id(),
        allocation
    );
    assert_eq!(source.snapshot().used(Class::Frame), 0);
    for _ in 0..1000 {
        let mut warm = destination.reserve_working_set(Class::Frame, 0)?;
        table.clear_retaining_capacity();
        table.reserve_reserved(&mut warm, 1)?;
        assert_eq!(table.insert_reserved(5, 17), None);
        assert_eq!(table.insert_reserved(5, 23), Some(17));
        assert_eq!(
            std::ptr::from_ref(table.get(&5).ok_or("warm entry")?),
            address
        );
    }
    // Growth recycles the old allocation only after its entries have moved.
    let growth_policy = AssetReadBudget::for_class(source.clone(), Class::Frame);
    let mut output = CpuBuffer::<u64>::default();
    let mut plan = CpuStorageWorkingSet::default();
    plan.include(
        table.reservation_bytes(&growth_policy, 64)?,
        table.replacement_credit(64),
    )?;
    plan.include(output.reservation_bytes(&source, Class::Frame, 64)?, 0)?;
    let mut fund = source.reserve_working_set(Class::Frame, plan.bytes())?;
    table.reserve_reserved(&mut fund, 64)?;
    output.reserve_reserved(&mut fund, CpuStorageKind::Result, 64)?;
    assert_eq!(table.get(&5), Some(&23));
    assert_ne!(
        table.memory.as_ref().ok_or("grown table")?.allocation_id(),
        allocation
    );
    drop((table, output, fund, adoption));
    assert_eq!(source.snapshot().used(Class::Frame), 0);
    assert_eq!(destination.snapshot().used(Class::Frame), 0);
    Ok(())
}
