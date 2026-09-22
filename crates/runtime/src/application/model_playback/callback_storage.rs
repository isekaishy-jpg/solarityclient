//! One frame-owned callback queue and frozen pose bank, admitted before scene mutation.
use super::PoseClockScratch;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use solarity_rendering::M2QueuedCallback;

#[derive(Default)]
pub(in crate::application) struct M2CallbackScratch {
    pub(super) clocks: PoseClockScratch,
    pub(super) queue: CpuBuffer<M2QueuedCallback>,
}

pub(in crate::application) struct M2CallbackStorage<'a> {
    pub(in crate::application) budget: &'a CpuStorageBudget,
    pub(in crate::application) scratch: &'a mut M2CallbackScratch,
    pub(in crate::application) queue_capacity: usize,
}

impl M2CallbackScratch {
    pub(in crate::application) fn include_storage(
        &self,
        budget: &CpuStorageBudget,
        bones: usize,
        callbacks: usize,
        plan: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        self.clocks.include_storage(budget, bones, plan)?;
        plan.include(
            self.queue
                .reservation_bytes(budget, Class::Frame, callbacks)?,
            self.queue.replacement_credit(callbacks),
        )
    }
    pub(in crate::application) fn reserve_reserved(
        &mut self,
        fund: &mut CpuStorageReservation,
        bones: usize,
        callbacks: usize,
    ) -> Result<(), CpuError> {
        self.clocks.reserve_reserved(fund, bones)?;
        self.queue.reserve_reserved(fund, Kind::Scratch, callbacks)
    }
    pub(super) fn prepare(
        &mut self,
        budget: &CpuStorageBudget,
        bones: usize,
        callbacks: usize,
    ) -> Result<(), CpuError> {
        let mut plan = CpuStorageWorkingSet::default();
        self.include_storage(budget, bones, callbacks, &mut plan)?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.reserve_reserved(&mut fund, bones, callbacks)
    }
}
