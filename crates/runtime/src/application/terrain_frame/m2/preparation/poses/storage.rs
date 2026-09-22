//! Admit all known pose staging/return metadata before capturing sampling inputs.
use solarity_cpu::{
    CpuBuffer, CpuError, CpuExecutor, CpuStorageBudget, CpuStorageClass as Class,
    CpuStorageKind as Kind, CpuStorageWorkingSet,
};

/// Production always supplies an executor. Only independent serial fixtures may omit it.
pub(in crate::application::terrain_frame::m2::preparation) fn budget(
    cpu: Option<&CpuExecutor>,
) -> Result<CpuStorageBudget, CpuError> {
    if let Some(cpu) = cpu {
        return Ok(cpu.storage().clone());
    }
    #[cfg(test)]
    {
        Ok(CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(
            usize::MAX,
            0,
            0,
        )))
    }
    #[cfg(not(test))]
    {
        Err(CpuError::InvalidExecutionPlan)
    }
}

pub(super) fn capacity<T>(buffer: &CpuBuffer<T>, needed: usize) -> Result<usize, CpuError> {
    if needed <= buffer.capacity() {
        return Ok(needed);
    }
    needed
        .checked_next_power_of_two()
        .ok_or(CpuError::StorageSizeOverflow)
}

impl super::PoseBatch {
    pub(in crate::application::terrain_frame::m2) fn prepare_storage(
        &mut self,
        budget: &CpuStorageBudget,
        placements: usize,
        maximum: usize,
    ) -> Result<(), CpuError> {
        let mut plan = CpuStorageWorkingSet::default();
        plan.include(
            self.indices
                .reservation_bytes(budget, Class::Frame, placements)?,
            self.indices.replacement_credit(placements),
        )?;
        plan.include(
            self.jobs.reservation_bytes(budget, Class::Frame, maximum)?,
            self.jobs.replacement_credit(maximum),
        )?;
        plan.include(
            self.handles
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.handles.replacement_credit(maximum),
        )?;
        plan.include(
            self.costs
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.costs.replacement_credit(maximum),
        )?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.indices
            .reserve_reserved(&mut fund, Kind::Metadata, placements)?;
        self.jobs
            .reserve_reserved(&mut fund, Kind::Result, maximum)?;
        self.handles
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        self.costs
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pose_metadata_refuses_as_a_group_and_reuses_full_capacity() -> Result<(), CpuError> {
        use super::super::{PoseBatch, input::PoseJob};
        let bytes = 10 * size_of::<Option<usize>>()
            + 3 * (size_of::<PoseJob>()
                + size_of::<solarity_cpu::FrameJob<PoseJob>>()
                + size_of::<solarity_cpu::JobCost>());
        let denied = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes - 1, 0, 0));
        let mut batch = PoseBatch::default();
        assert!(batch.prepare_storage(&denied, 10, 3).is_err());
        assert_eq!(batch.indices.capacity(), 0);
        assert_eq!(batch.jobs.capacity(), 0);
        assert_eq!(batch.handles.capacity(), 0);
        assert_eq!(batch.costs.capacity(), 0);
        assert_eq!(denied.snapshot().used(Class::Frame), 0);
        let budget = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes, 0, 0));
        batch.prepare_storage(&budget, 10, 3)?;
        batch.indices.resize_with(10, || None)?;
        batch.indices[7] = Some(2);
        let addresses = (
            batch.jobs.as_ptr(),
            batch.indices.as_ptr(),
            batch.handles.as_ptr(),
            batch.costs.as_ptr(),
        );
        batch.prepare_storage(&budget, 10, 3)?;
        assert_eq!(
            addresses,
            (
                batch.jobs.as_ptr(),
                batch.indices.as_ptr(),
                batch.handles.as_ptr(),
                batch.costs.as_ptr()
            )
        );
        assert_eq!(batch.indices[7], Some(2));
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        assert!(batch.prepare_storage(&denied, 10, 3).is_err());
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        assert_eq!(batch.indices[7], Some(2));
        drop(batch);
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
