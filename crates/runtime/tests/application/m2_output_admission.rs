//! Renderer-vector adoption and replacement obey CPU old-plus-new accounting.

use super::reserve;
use solarity_cpu::{CpuStorageBudget, CpuStorageClass as Class, CpuStoragePlan};
use std::error::Error;

#[test]
fn adoption_and_refused_growth_keep_values_and_the_original_charge() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(96, 0, 0));
    let mut values = Vec::with_capacity(4);
    values.extend([10_u64, 20, 30, 40]);
    let mut charge = None;
    reserve(&mut values, &mut charge, &budget, 0)?;
    let old_bytes = values.capacity() * size_of::<u64>();
    let old_identity = charge.as_ref().ok_or("charge")?.allocation_id();
    assert_eq!(budget.snapshot().used(Class::Frame), old_bytes);
    // A 12-element replacement plus the retained four-element buffer cannot fit.
    assert!(reserve(&mut values, &mut charge, &budget, 8).is_err());
    assert_eq!(values, [10, 20, 30, 40]);
    assert_eq!(values.capacity() * size_of::<u64>(), old_bytes);
    assert_eq!(
        charge.as_ref().ok_or("charge")?.allocation_id(),
        old_identity
    );
    assert_eq!(budget.snapshot().used(Class::Frame), old_bytes);
    let larger = CpuStorageBudget::new(CpuStoragePlan::new(256, 0, 0));
    reserve(&mut values, &mut charge, &larger, 8)?;
    assert_eq!(values, [10, 20, 30, 40]);
    assert!(values.capacity() >= 12);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(
        larger.snapshot().used(Class::Frame),
        values.capacity() * size_of::<u64>()
    );
    assert_ne!(
        charge.as_ref().ok_or("charge")?.allocation_id(),
        old_identity
    );
    drop(values);
    drop(charge);
    assert_eq!(larger.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn refused_executor_transfer_keeps_the_previous_storage_owner() -> Result<(), Box<dyn Error>> {
    let original = CpuStorageBudget::new(CpuStoragePlan::new(256, 0, 0));
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    let mut values = vec![7_u64, 8, 9];
    let mut charge = None;
    reserve(&mut values, &mut charge, &original, 0)?;
    let identity = charge.as_ref().ok_or("charge")?.allocation_id();
    assert!(reserve(&mut values, &mut charge, &refused, 0).is_err());
    assert_eq!(values, [7, 8, 9]);
    assert_eq!(charge.as_ref().ok_or("charge")?.allocation_id(), identity);
    assert_eq!(refused.snapshot().used(Class::Frame), 0);
    assert_eq!(
        original.snapshot().used(Class::Frame),
        values.capacity() * size_of::<u64>()
    );
    Ok(())
}
