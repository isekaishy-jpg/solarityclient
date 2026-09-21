//! Pressure and final-owner tests for the common source readiness bridge.
use super::*;
use solarity_cpu::{CpuService, CpuStorageKind, CpuStoragePlan};
use std::error::Error;

#[test]
fn source_metadata_refusal_does_not_change_demand_or_strand_a_dependency()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let slot = SourceSlot::<u32, &'static str>::new(Some(&budget))?;
    let baseline = budget.snapshot().used(CpuStorageClass::Required);
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - baseline,
    )?;
    assert!(slot.subscribe(CpuService::Required).is_err());
    assert_eq!(slot.demand.strongest(), None);
    assert!(SourceSlot::<u32, &'static str>::new(Some(&budget)).is_err());
    drop(hold);
    let interest = slot.subscribe(CpuService::Required)?;
    let before = budget.snapshot().used(CpuStorageClass::Required);
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - before,
    )?;
    assert!(
        slot.dependency(
            &budget,
            CpuStorageClass::Required,
            interest.clone(),
            || "abandoned"
        )
        .is_err()
    );
    drop(hold);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), before);
    let edge = slot.dependency(
        &budget,
        CpuStorageClass::Required,
        interest.clone(),
        || "abandoned",
    )?;
    *slot.result.lock().map_err(|_| "result lock")? = Some(Ok(73));
    slot.publish_dependencies();
    assert_eq!(edge.poll(), Some(Ok(73)));
    drop((slot, interest));
    assert!(budget.snapshot().used(CpuStorageClass::Required) > 0);
    assert_eq!(edge.poll(), Some(Ok(73)));
    drop(edge);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn abandoned_weak_listener_retains_its_allocation_charge_until_registration_retires()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(65536, 65536, 0));
    let slot = SourceSlot::<u32, &'static str>::new(Some(&budget))?;
    let interest = slot.subscribe(CpuService::Required)?;
    let edge = slot.dependency(
        &budget,
        CpuStorageClass::Frame,
        interest.clone(),
        || "abandoned",
    )?;
    drop(edge);
    // The listener owns a weak Arc even though the consumer and publisher retired.
    assert!(
        budget.snapshot().used(CpuStorageClass::Frame)
            >= size_of::<DependencyOwner<u32, &'static str>>()
    );
    *slot.result.lock().map_err(|_| "result lock")? = Some(Err("source error"));
    slot.publish_dependencies();
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    drop((slot, interest));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
