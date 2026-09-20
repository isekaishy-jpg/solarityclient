//! Visibility permutations must not spread large output capacities across slots.

use super::{GeometryJob, GeometryReuse, GeometryReuseIdentity, ReuseKey};
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

/// One emitter crosses every ordinal while the same small models remain visible.
/// Ordinal-based reuse exhausts this budget after the emitter visits a few slots.
#[test]
fn moving_emitter_keeps_one_large_output_allocation() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(48 * 1024, 0, 0));
    let mut jobs = Vec::new();
    let mut reuse = GeometryReuse::default();
    let mut retained = 0;
    for frame in 0..256 {
        reuse.index(&mut jobs);
        let mut completed = Vec::new();
        for ordinal in 0..32 {
            let generation = (ordinal + frame) % 32;
            let identity = identity(generation, true);
            let slot = reuse.take(&identity, &mut jobs);
            let job = &mut jobs[slot];
            job.reuse_identity = Some(identity);
            job.particle_indices.reserve(
                &budget,
                Class::Frame,
                Kind::Result,
                if generation == 0 { 8192 } else { 4 },
            )?;
            completed.push(std::mem::take(job));
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
    drop(jobs);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

/// Shadow-only demand must not inherit visible particle buffers, and a retired
/// immutable generation must not supply storage through a recycled draw ordinal.
#[test]
fn changed_demand_and_generation_retire_unmatched_storage() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(65536, 0, 0));
    let mut job = GeometryJob {
        reuse_identity: Some(identity(7, true)),
        ..GeometryJob::default()
    };
    job.particle_indices
        .reserve(&budget, Class::Frame, Kind::Result, 8192)?;
    let mut jobs = vec![job];
    let mut reuse = GeometryReuse::default();
    reuse.index(&mut jobs);
    let shadow = reuse.take(&identity(7, false), &mut jobs);
    assert_eq!(jobs[shadow].particle_indices.capacity(), 0);
    let replacement = reuse.take(&identity(8, true), &mut jobs);
    assert_eq!(jobs[replacement].particle_indices.capacity(), 0);
    let completed = [
        std::mem::take(&mut jobs[shadow]),
        std::mem::take(&mut jobs[replacement]),
    ];
    jobs.clear();
    reuse.clear();
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(completed.len(), 2);
    Ok(())
}

/// Repeated placements of one source each own a distinct output allocation.
#[test]
fn duplicate_source_placements_reuse_each_slot_once() {
    let mut jobs = (0..3)
        .map(|_| GeometryJob {
            reuse_identity: Some(identity(7, true)),
            ..GeometryJob::default()
        })
        .collect::<Vec<_>>();
    let mut reuse = GeometryReuse::default();
    reuse.index(&mut jobs);
    let mut slots = (0..4)
        .map(|_| reuse.take(&identity(7, true), &mut jobs))
        .collect::<Vec<_>>();
    slots.sort_unstable();
    assert_eq!(slots, [0, 1, 2, 3]);
}
