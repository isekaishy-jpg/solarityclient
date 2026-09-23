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

#[test]
fn complete_geometry_publication_is_atomic_and_preserves_each_group() -> Result<(), Box<dyn Error>>
{
    use solarity_cpu::{CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, FrameBatch};
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(2, 1, 1, 1)?,
        std::num::NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(4 << 20, 4 << 20, 0),
    ))?;
    let budget = cpu.storage().clone();
    let baseline = budget.snapshot().used(CpuStorageClass::Frame);
    for closed in [false, true] {
        let mut batch = GeometryBatch {
            pending: FrameBatch::new(|chunk: &mut GeometryChunk| {
                for owner in chunk.jobs.iter_mut() {
                    owner.job_mut().particle_vertex_capacity += 1;
                }
            }),
            ..Default::default()
        };
        batch.prepare_scratch(&cpu)?;
        batch.begin_metadata(&cpu, 3)?;
        batch.submitted = true;
        for ordinal in 0..3 {
            batch.staged.reserve(&budget)?;
            let mut owner = GeometryOwner::new(&budget)?;
            owner.job_mut().particle_index_capacity = ordinal;
            batch.staged.push(owner, cost(ordinal as u64 + 1));
            batch.flush_staged()?;
            assert!(matches!(batch.pending.job(0), Err(CpuError::InvalidJob)));
        }
        assert_eq!(batch.owned_chunks.len(), 3);
        let pointers: Vec<_> = batch
            .owned_chunks
            .iter()
            .map(|chunk| chunk.jobs.as_ptr())
            .collect();
        let pressure = budget.reserve(
            CpuStorageClass::Frame,
            CpuStorageKind::Scratch,
            budget.snapshot().limit(CpuStorageClass::Frame)
                - budget.snapshot().used(CpuStorageClass::Frame),
        )?;
        if closed {
            batch.pending.close();
        }
        let published = batch.publish_groups();
        if closed {
            assert!(matches!(
                published,
                Err(
                    crate::application::terrain_frame::RuntimeTerrainFrameError::Cpu(
                        CpuError::BatchClosed
                    )
                )
            ));
            assert_eq!(batch.owned_chunks.len(), 3);
            assert_eq!(batch.chunk_costs.len(), 3);
            assert!(matches!(batch.pending.job(0), Err(CpuError::InvalidJob)));
        } else {
            published?;
            assert!(batch.owned_chunks.is_empty());
            assert!(batch.chunk_costs.is_empty());
        }
        batch
            .pending
            .reclaim_into(&mut batch.owned_chunks.writer())?;
        for (ordinal, chunk) in batch.owned_chunks.iter().enumerate() {
            assert_eq!(chunk.jobs.as_ptr(), pointers[ordinal]);
            assert_eq!(chunk.jobs[0].job().particle_index_capacity, ordinal);
            assert_eq!(
                chunk.jobs[0].job().particle_vertex_capacity,
                usize::from(!closed)
            );
        }
        drop((batch, pressure));
        assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), baseline);
    }
    cpu.shutdown()?;
    Ok(())
}
