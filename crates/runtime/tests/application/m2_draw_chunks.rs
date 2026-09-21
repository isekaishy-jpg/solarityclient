//! Draw grouping bounds scheduling overhead without weakening ownership on refusal.

use super::{GeometryBatch, GeometryChunk, GeometryOwner, MAX_MODELS};
use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuStoragePlan, JobCost};
use std::{error::Error, time::Duration};

fn cost(micros: u64) -> JobCost {
    JobCost::measured(Duration::from_micros(micros))
}

#[test]
fn unknown_and_large_draw_work_stay_indivisible_while_small_models_share_dispatch()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(4 << 20, 0, 0));
    let mut chunk = GeometryChunk::default();
    chunk.reserve(&budget)?;
    let mut returned = Vec::new();
    chunk.push(GeometryOwner::new(&budget)?, JobCost::default());
    assert!(chunk.full());
    assert!(chunk.precedes(cost(1)));
    assert!(chunk.cost().duration().is_none());
    chunk.reclaim(&mut returned)?;
    chunk.push(GeometryOwner::new(&budget)?, cost(60));
    assert!(!chunk.precedes(cost(40)));
    assert!(chunk.precedes(cost(41)));
    assert!(chunk.precedes(JobCost::default()));
    chunk.push(GeometryOwner::new(&budget)?, cost(40));
    assert!(chunk.full());
    assert_eq!(chunk.cost().duration(), Some(Duration::from_micros(100)));
    chunk.reclaim(&mut returned)?;
    for _ in 0..MAX_MODELS {
        chunk.push(GeometryOwner::new(&budget)?, cost(1));
    }
    assert!(chunk.full());
    assert!(chunk.precedes(cost(1)));
    chunk.reclaim(&mut returned)?;
    chunk.push(GeometryOwner::new(&budget)?, cost(2_000));
    assert!(chunk.full());
    assert!(chunk.precedes(cost(1)));
    assert_eq!(chunk.jobs.len(), 1);
    Ok(())
}

#[test]
fn refused_chunk_submission_keeps_every_unexecuted_model_and_its_charge()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(4 << 20, 0, 0));
    let mut batch = GeometryBatch::default();
    batch.staged.reserve(&budget)?;
    for marker in ["first", "second"] {
        let mut owner = GeometryOwner::new(&budget)?;
        owner.job_mut().recoverable_errors.reserve(
            &budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Result,
            1,
        )?;
        owner.job_mut().recoverable_errors.push(marker.to_owned())?;
        batch.staged.push(owner, cost(10));
    }
    let charged = budget
        .snapshot()
        .bytes(CpuStorageClass::Frame, CpuStorageKind::Scratch);
    assert!(charged > 0);
    // No active producer: refusal must restore the staged group's exact ownership.
    assert!(batch.flush_staged().is_err());
    assert_eq!(batch.staged.jobs.len(), 2);
    assert_eq!(&*batch.staged.jobs[0].job().recoverable_errors, &["first"]);
    assert_eq!(&*batch.staged.jobs[1].job().recoverable_errors, &["second"]);
    assert_eq!(
        budget
            .snapshot()
            .bytes(CpuStorageClass::Frame, CpuStorageKind::Scratch),
        charged
    );
    let denied = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    assert!(batch.staged.reserve(&denied).is_err());
    assert_eq!(batch.staged.jobs.len(), 2);
    drop(batch);
    assert_eq!(
        budget
            .snapshot()
            .bytes(CpuStorageClass::Frame, CpuStorageKind::Scratch),
        0
    );
    Ok(())
}
