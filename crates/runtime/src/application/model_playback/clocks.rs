//! Main-owned clock copies survive ordered callback and frame-continuation boundaries.
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use solarity_rendering::M2AnimationClock;

#[derive(Default)]
pub(in crate::application) struct PoseClockScratch {
    values: CpuBuffer<(u16, M2AnimationClock)>,
}
impl PoseClockScratch {
    pub(in crate::application) fn include_storage(
        &self,
        budget: &CpuStorageBudget,
        count: usize,
        plan: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        plan.include(
            self.values.reservation_bytes(budget, Class::Frame, count)?,
            self.values.replacement_credit(count),
        )
    }
    pub(in crate::application) fn reserve_reserved(
        &mut self,
        fund: &mut CpuStorageReservation,
        count: usize,
    ) -> Result<(), CpuError> {
        self.values.reserve_reserved(fund, Kind::Scratch, count)
    }
    /// Reserve before a callback scan can change timers or consume random values.
    pub(in crate::application) fn prepare(
        &mut self,
        budget: &CpuStorageBudget,
        count: usize,
    ) -> Result<(), CpuError> {
        self.values
            .reserve(budget, Class::Frame, Kind::Scratch, count)
    }
    pub(in crate::application) fn capture(
        &mut self,
        budget: &CpuStorageBudget,
        clocks: impl Iterator<Item = (u16, M2AnimationClock)>,
    ) -> Result<&[(u16, M2AnimationClock)], CpuError> {
        let count = clocks.size_hint().1.ok_or(CpuError::StorageSizeOverflow)?;
        self.prepare(budget, count)?;
        self.values.clear();
        for clock in clocks {
            self.values.push(clock)?;
        }
        Ok(&self.values)
    }
    pub(in crate::application) fn values(&self) -> &[(u16, M2AnimationClock)] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_capture_keeps_prior_values_on_refusal_and_reuses_warm_storage() -> Result<(), CpuError>
    {
        let bytes = 2 * size_of::<(u16, M2AnimationClock)>();
        let budget = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes, 0, 0));
        let denied = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes - 1, 0, 0));
        let first = (3, M2AnimationClock::new(0, 100., 100.));
        let second = (5, M2AnimationClock::new(0, 120., 120.));
        let values = [first, second];
        let mut scratch = PoseClockScratch::default();
        assert!(scratch.capture(&denied, values.into_iter()).is_err());
        assert_eq!(scratch.values.capacity(), 0);
        assert_eq!(
            scratch.capture(&budget, values.into_iter().filter(|_| true))?,
            values
        );
        let address = scratch.values.as_ptr();
        assert!(scratch.capture(&budget, [first; 3].into_iter()).is_err());
        assert_eq!(scratch.values(), values);
        assert!(scratch.capture(&denied, values.into_iter()).is_err());
        assert_eq!(scratch.values(), values);
        assert!(matches!(
            scratch.capture(&budget, std::iter::repeat(first)),
            Err(CpuError::StorageSizeOverflow)
        ));
        assert_eq!(scratch.values(), values);
        for _ in 0..100 {
            assert_eq!(
                scratch.capture(&budget, [second, first].into_iter())?,
                [second, first]
            );
            assert_eq!(scratch.values.as_ptr(), address);
        }
        drop(scratch);
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
