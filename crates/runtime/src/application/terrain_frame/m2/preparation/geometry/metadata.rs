//! Admit retained dispatch/reclamation arrays before any model can leave main.
use super::GeometryBatch;
use solarity_cpu::{
    CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageWorkingSet,
};
impl GeometryBatch {
    pub(super) fn prepare_metadata(
        &mut self,
        budget: &CpuStorageBudget,
        maximum: usize,
    ) -> Result<(), CpuError> {
        // Unmatched prior jobs coexist with placeholders/new jobs until phase return.
        let jobs = self
            .jobs
            .len()
            .checked_add(maximum)
            .ok_or(CpuError::StorageSizeOverflow)?;
        let spares = maximum.max(self.spare_chunks.len());
        let mut plan = CpuStorageWorkingSet::default();
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
            self.returned_chunks
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.returned_chunks.replacement_credit(maximum),
        )?;
        let mut reservation = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.jobs
            .reserve_reserved(&mut reservation, Kind::Metadata, jobs)?;
        self.spare_chunks
            .reserve_reserved(&mut reservation, Kind::Metadata, spares)?;
        self.returned_chunks
            .reserve_reserved(&mut reservation, Kind::Metadata, maximum)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_return_storage_is_admitted_before_any_array_grows() -> Result<(), CpuError> {
        let bytes = 3 * size_of::<super::super::GeometryOwner>()
            + 6 * size_of::<super::super::chunk::GeometryChunk>();
        let refused = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes - 1, 0, 0));
        let mut batch = GeometryBatch::default();
        assert!(batch.prepare_metadata(&refused, 3).is_err());
        assert_eq!(batch.jobs.capacity(), 0);
        assert_eq!(batch.spare_chunks.capacity(), 0);
        assert_eq!(batch.returned_chunks.capacity(), 0);
        assert_eq!(refused.snapshot().used(Class::Frame), 0);
        let budget = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes, 0, 0));
        batch.prepare_metadata(&budget, 3)?;
        assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Metadata), bytes);
        let pointers = (
            batch.jobs.as_ptr(),
            batch.spare_chunks.as_ptr(),
            batch.returned_chunks.as_ptr(),
        );
        batch.prepare_metadata(&budget, 3)?;
        assert_eq!(
            (
                batch.jobs.as_ptr(),
                batch.spare_chunks.as_ptr(),
                batch.returned_chunks.as_ptr()
            ),
            pointers
        );
        drop(batch);
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
