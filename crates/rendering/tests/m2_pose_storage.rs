//! Nested skeletal allocations remain charged across worker/result ownership.

use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStoragePlan};
use solarity_rendering::M2BonePose;
use std::error::Error;

/// A pose carries its charge through swaps and drops it only with its actual arrays.
#[test]
fn pose_storage_moves_with_the_palette_and_releases_on_drop() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20));
    let mut producer = M2BonePose::default();
    producer.reserve_cpu_storage(&budget, 128)?;
    let bytes = producer.allocated_bytes();
    assert!(bytes > 128 * 64);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), bytes);
    let mut consumer = M2BonePose::default();
    std::mem::swap(&mut producer, &mut consumer);
    drop(producer);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), bytes);
    consumer.reserve_cpu_storage(&budget, 64)?;
    assert_eq!(consumer.allocated_bytes(), bytes);
    drop(consumer);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}

/// Failed growth leaves the complete old capacity and its reservation usable.
#[test]
fn refused_pose_growth_preserves_retained_storage() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(16 << 10, 1 << 20, 1 << 20));
    let mut pose = M2BonePose::default();
    pose.reserve_cpu_storage(&budget, 16)?;
    let bytes = pose.allocated_bytes();
    assert!(pose.reserve_cpu_storage(&budget, 10_000).is_err());
    assert_eq!(pose.allocated_bytes(), bytes);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), bytes);
    pose.reserve_cpu_storage(&budget, 16)?;
    drop(pose);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}
