//! Admit retained dispatch/reclamation arrays before any model can leave main.
use super::GeometryBatch;
use solarity_cpu::{
    CpuError, CpuExecutor, CpuStorageClass as Class, CpuStorageKind as Kind, CpuStorageWorkingSet,
};
impl GeometryBatch {
    pub(super) fn begin_metadata(
        &mut self,
        cpu: &CpuExecutor,
        maximum: usize,
    ) -> Result<(), CpuError> {
        let budget = cpu.storage();
        // Unmatched prior jobs coexist with placeholders/new jobs until phase return.
        let jobs = self
            .jobs
            .len()
            .checked_add(maximum)
            .ok_or(CpuError::StorageSizeOverflow)?;
        let spares = maximum.max(self.spare_chunks.len());
        let mut plan = CpuStorageWorkingSet::default();
        let reuse_count = self.jobs.len();
        let policy = solarity_asset::AssetReadBudget::for_class(budget.clone(), Class::Frame);
        plan.include(
            self.reuse.heads.reservation_bytes(&policy, reuse_count)?,
            self.reuse.heads.replacement_credit(reuse_count),
        )?;
        plan.include(
            self.jobs.reservation_bytes(budget, Class::Frame, jobs)?,
            self.jobs.replacement_credit(jobs),
        )?;
        plan.include(
            self.spare_chunks
                .reservation_bytes(budget, Class::Frame, spares)?,
            self.spare_chunks.replacement_credit(spares),
        )?;
        plan.include(
            self.owned_chunks
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.owned_chunks.replacement_credit(maximum),
        )?;
        plan.include(
            self.chunk_costs
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.chunk_costs.replacement_credit(maximum),
        )?;
        let Self {
            reuse,
            jobs: owners,
            spare_chunks,
            owned_chunks,
            chunk_costs,
            pending,
            ..
        } = self;
        pending.begin_with_storage(
            cpu,
            solarity_cpu::FrameBatchPlan::new(maximum, 0),
            &[],
            plan.bytes(),
            |reservation| {
                reuse.heads.reserve_reserved(reservation, reuse_count)?;
                owners.reserve_reserved(reservation, Kind::Metadata, jobs)?;
                spare_chunks.reserve_reserved(reservation, Kind::Metadata, spares)?;
                owned_chunks.reserve_reserved(reservation, Kind::Metadata, maximum)?;
                chunk_costs.reserve_reserved(reservation, Kind::Metadata, maximum)?;
                Ok(())
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_return_storage_and_scheduler_are_admitted_before_any_array_grows()
    -> Result<(), CpuError> {
        let mut cpu = CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
            solarity_cpu::CpuExecutionPlan::new(0, 1, 1, 1)?,
            std::num::NonZeroUsize::MIN,
            solarity_cpu::CpuStoragePlan::new(1 << 20, 1 << 20, 0),
        ))?;
        let budget = cpu.storage().clone();
        let baseline = budget.snapshot().used(Class::Frame);
        let bytes = 3 * size_of::<super::super::GeometryOwner>()
            + 6 * size_of::<super::super::chunk::GeometryChunk>()
            + 3 * size_of::<solarity_cpu::JobCost>();
        let pressure = budget.reserve(
            Class::Frame,
            Kind::Scratch,
            budget.snapshot().limit(Class::Frame) - baseline - bytes,
        )?;
        let held = budget.snapshot().used(Class::Frame);
        let mut batch = GeometryBatch::default();
        assert!(matches!(
            batch.begin_metadata(&cpu, 3),
            Err(CpuError::StorageAtCapacity { .. })
        ));
        assert_eq!(batch.jobs.capacity(), 0);
        assert_eq!(batch.spare_chunks.capacity(), 0);
        assert_eq!(batch.owned_chunks.capacity(), 0);
        assert_eq!(batch.chunk_costs.capacity(), 0);
        assert_eq!(budget.snapshot().used(Class::Frame), held);
        assert!(matches!(
            batch.pending.completion(),
            Err(CpuError::BatchInactive)
        ));
        drop(pressure);
        batch.begin_metadata(&cpu, 3)?;
        batch.pending.close();
        batch
            .pending
            .reclaim_into(&mut batch.owned_chunks.writer())?;
        let pointers = (
            batch.jobs.as_ptr(),
            batch.spare_chunks.as_ptr(),
            batch.owned_chunks.as_ptr(),
            batch.chunk_costs.as_ptr(),
        );
        let pressure = budget.reserve(
            Class::Frame,
            Kind::Scratch,
            budget.snapshot().limit(Class::Frame) - budget.snapshot().used(Class::Frame),
        )?;
        batch.begin_metadata(&cpu, 3)?;
        batch.pending.close();
        batch
            .pending
            .reclaim_into(&mut batch.owned_chunks.writer())?;
        assert_eq!(
            (
                batch.jobs.as_ptr(),
                batch.spare_chunks.as_ptr(),
                batch.owned_chunks.as_ptr(),
                batch.chunk_costs.as_ptr()
            ),
            pointers
        );
        drop((batch, pressure));
        cpu.shutdown()?;
        assert_eq!(budget.snapshot().used(Class::Frame), baseline);
        Ok(())
    }
}
