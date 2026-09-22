//! Visibility permutations must not spread large output capacities across slots.

use super::{GeometryOwner, GeometryReuse, GeometryReuseIdentity, ReuseKey};
use solarity_cpu::{
    CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan,
};
use std::{error::Error, sync::Weak};

/// Controlled generation tokens isolate reuse from scene/GPU resource creation.
fn identity(generation: usize, visible: bool) -> GeometryReuseIdentity {
    GeometryReuseIdentity {
        key: ReuseKey {
            generation,
            visible,
            shadows: true,
        },
        _generation: Weak::new(),
    }
}

fn owners(
    budget: &CpuStorageBudget,
    count: usize,
) -> Result<solarity_cpu::CpuBuffer<GeometryOwner>, Box<dyn Error>> {
    let mut jobs = solarity_cpu::CpuBuffer::default();
    jobs.reserve(budget, Class::Frame, Kind::Metadata, count)?;
    Ok(jobs)
}

/// One emitter crosses every ordinal while the same small models remain visible.
/// Ordinal-based reuse exhausts this budget after the emitter visits a few slots.
#[test]
fn moving_emitter_keeps_one_large_output_allocation() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(
        48 * 1024 + 32 * std::mem::size_of::<super::super::GeometryJob>(),
        0,
        0,
    ));
    let mut jobs = owners(&budget, 32)?;
    let mut reuse = GeometryReuse::default();
    reuse.heads.reserve(
        Some(&solarity_asset::AssetReadBudget::for_class(
            budget.clone(),
            Class::Frame,
        )),
        32,
    )?;
    let mut retained = 0;
    for frame in 0..256 {
        reuse.index(&mut jobs);
        let mut completed = owners(&budget, 32)?;
        for ordinal in 0..32 {
            let generation = (ordinal + frame) % 32;
            let identity = identity(generation, true);
            let slot = reuse.take(&identity, &mut jobs, &budget)?;
            let job = jobs[slot].job_mut();
            job.reuse_identity = Some(identity);
            job.particle_indices.reserve(
                &budget,
                Class::Frame,
                Kind::Result,
                if generation == 0 { 8192 } else { 4 },
            )?;
            completed.push(std::mem::take(&mut jobs[slot]))?;
        }
        jobs.clear();
        reuse.clear();
        jobs = completed;
        let used = budget.snapshot().used(Class::Frame);
        if frame == 0 {
            retained = used;
        }
        assert_eq!(used, retained, "frame {frame} changed only traversal order");
    }
    drop((jobs, reuse));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

/// Shadow-only demand must not inherit visible particle buffers, and a retired
/// immutable generation must not supply storage through a recycled draw ordinal.
#[test]
fn changed_demand_and_generation_retire_unmatched_storage() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(65536, 0, 0));
    let mut job = GeometryOwner::new(&budget)?;
    job.job_mut().reuse_identity = Some(identity(7, true));
    job.job_mut()
        .particle_indices
        .reserve(&budget, Class::Frame, Kind::Result, 8192)?;
    let mut jobs = owners(&budget, 3)?;
    jobs.push(job)?;
    let mut reuse = GeometryReuse::default();
    reuse.heads.reserve(
        Some(&solarity_asset::AssetReadBudget::for_class(
            budget.clone(),
            Class::Frame,
        )),
        1,
    )?;
    reuse.index(&mut jobs);
    let shadow = reuse.take(&identity(7, false), &mut jobs, &budget)?;
    assert_eq!(jobs[shadow].job().particle_indices.capacity(), 0);
    let replacement = reuse.take(&identity(8, true), &mut jobs, &budget)?;
    assert_eq!(jobs[replacement].job().particle_indices.capacity(), 0);
    let completed = [
        std::mem::take(&mut jobs[shadow]),
        std::mem::take(&mut jobs[replacement]),
    ];
    jobs.clear();
    reuse.clear();
    assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Result), 0);
    assert_eq!(completed.len(), 2);
    drop((completed, jobs, reuse));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

/// Repeated placements of one source each own a distinct output allocation.
#[test]
fn duplicate_source_placements_reuse_each_slot_once() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 0, 0));
    let mut jobs = owners(&budget, 4)?;
    for _ in 0..3 {
        let mut owner = GeometryOwner::new(&budget)?;
        owner.job_mut().reuse_identity = Some(identity(7, true));
        jobs.push(owner)?;
    }
    let mut reuse = GeometryReuse::default();
    reuse.heads.reserve(
        Some(&solarity_asset::AssetReadBudget::for_class(
            budget.clone(),
            Class::Frame,
        )),
        1,
    )?;
    reuse.index(&mut jobs);
    let mut slots = Vec::new();
    for _ in 0..4 {
        slots.push(reuse.take(&identity(7, true), &mut jobs, &budget)?);
    }
    slots.sort_unstable();
    assert_eq!(slots, [0, 1, 2, 3]);
    Ok(())
}

/// A failed executor rebind must not consume a reusable generation's list entry.
#[test]
fn rejected_record_transfer_preserves_the_reusable_owner() -> Result<(), Box<dyn Error>> {
    let bytes =
        std::mem::size_of::<super::super::GeometryJob>() + std::mem::size_of::<GeometryOwner>();
    let source = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
    let denied = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    let mut owner = GeometryOwner::new(&source)?;
    owner.job_mut().reuse_identity = Some(identity(7, true));
    let address = std::ptr::from_ref(owner.job());
    let mut jobs = owners(&source, 1)?;
    jobs.push(owner)?;
    let mut reuse = GeometryReuse::default();
    let index_budget = CpuStorageBudget::new(CpuStoragePlan::new(4096, 0, 0));
    reuse.heads.reserve(
        Some(&solarity_asset::AssetReadBudget::for_class(
            index_budget,
            Class::Frame,
        )),
        1,
    )?;
    reuse.index(&mut jobs);
    assert!(reuse.take(&identity(7, true), &mut jobs, &denied).is_err());
    assert_eq!(jobs.len(), 1);
    assert_eq!(source.snapshot().used(Class::Frame), bytes);
    assert_eq!(reuse.take(&identity(7, true), &mut jobs, &source)?, 0);
    assert_eq!(std::ptr::from_ref(jobs[0].job()), address);
    Ok(())
}
