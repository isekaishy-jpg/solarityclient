//! Nested skeletal allocations remain charged across worker/result ownership.

use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStoragePlan};
use solarity_rendering::{M2BonePose, M2BoneSamples};
use std::error::Error;

/// Named sampling retains both its ancestor mask and nested palette scratch.
#[test]
fn named_samples_own_all_nested_capacity() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20));
    let mut samples = M2BoneSamples::default();
    samples.reserve_cpu_storage(&budget, 128)?;
    let bytes = samples.allocated_bytes();
    assert!(bytes > 128 * 128);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), bytes);
    samples.reserve_cpu_storage(&budget, 32)?;
    assert_eq!(samples.allocated_bytes(), bytes);
    drop(samples);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}

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

/// A budget fitting the palette alone must not partially grow a named-sample owner.
#[test]
fn named_samples_refuse_the_connected_set_before_palette_growth() -> Result<(), Box<dyn Error>> {
    let probe = CpuStorageBudget::new(CpuStoragePlan::new(usize::MAX, 0, 0));
    let mut plan = solarity_cpu::CpuStorageWorkingSet::default();
    M2BonePose::default().include_cpu_storage(&probe, 128, &mut plan)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(plan.bytes(), 0, 0));
    let mut samples = M2BoneSamples::default();
    assert!(samples.reserve_cpu_storage(&budget, 128).is_err());
    assert_eq!(samples.allocated_bytes(), 0);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}

/// A protected model allowance survives competing demand and supports zero-headroom reuse.
#[test]
fn palettes_and_effect_pools_share_one_protected_reservation() -> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuStorageKind, CpuStorageWorkingSet};
    use solarity_rendering::M2ParticleSimulation;
    let probe = CpuStorageBudget::new(CpuStoragePlan::new(usize::MAX, 0, 0));
    let mut pose = M2BonePose::default();
    let mut particles = M2ParticleSimulation::new(47);
    let mut plan = CpuStorageWorkingSet::default();
    pose.include_cpu_storage(&probe, 16, &mut plan)?;
    particles.include_cpu_storage(&probe, 32, &mut plan)?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(plan.bytes() + 64, 0, 0));
    let mut reservation = budget.reserve_working_set(CpuStorageClass::Frame, plan.bytes())?;
    let competitor = budget.reserve(CpuStorageClass::Frame, CpuStorageKind::Scratch, 64)?;
    pose.reserve_cpu_storage_reserved(&mut reservation, 16)?;
    assert!(
        budget
            .reserve(CpuStorageClass::Frame, CpuStorageKind::Scratch, 1)
            .is_err()
    );
    particles.reserve_cpu_storage_reserved(&mut reservation, 32)?;
    drop(reservation);
    assert_eq!(
        particles.capacity(),
        0,
        "physical admission preserves the stock live limit"
    );
    let retained = pose.allocated_bytes() + particles.allocated_bytes();
    assert_eq!(
        budget.snapshot().used(CpuStorageClass::Frame),
        retained + 64
    );
    let mut warm = CpuStorageWorkingSet::default();
    pose.include_cpu_storage(&budget, 16, &mut warm)?;
    particles.include_cpu_storage(&budget, 32, &mut warm)?;
    assert_eq!(warm.bytes(), 0);
    let mut reservation = budget.reserve_working_set(CpuStorageClass::Frame, 0)?;
    pose.reserve_cpu_storage_reserved(&mut reservation, 16)?;
    particles.reserve_cpu_storage_reserved(&mut reservation, 32)?;
    drop((reservation, competitor, pose, particles));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}
