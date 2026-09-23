//! Deferred event records and their independent pose snapshots share admission.
use super::M2ExpiredVariation;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use solarity_rendering::{M2AnimationClock, M2EventTimeWindow, M2QueuedCallback};

pub(super) struct EventOutput<'a> {
    budget: Option<&'a CpuStorageBudget>,
    values: CpuBuffer<M2ExpiredVariation>,
}

impl<'a> EventOutput<'a> {
    pub(super) fn required(budget: Option<&'a CpuStorageBudget>) -> Result<Self, CpuError> {
        let budget = match budget {
            Some(budget) => budget,
            #[cfg(test)]
            None => {
                static SERIAL: std::sync::OnceLock<CpuStorageBudget> = std::sync::OnceLock::new();
                SERIAL.get_or_init(|| {
                    CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(usize::MAX, 0, 0))
                })
            }
            #[cfg(not(test))]
            None => return Err(CpuError::InvalidExecutionPlan),
        };
        Ok(Self {
            budget: Some(budget),
            values: CpuBuffer::default(),
        })
    }

    /// Sky consumes only the resulting clock. It still runs native timer transitions.
    pub(super) fn discarded() -> Self {
        Self {
            budget: None,
            values: CpuBuffer::default(),
        }
    }

    fn reserve_group(
        &mut self,
        count: usize,
        bones: usize,
    ) -> Result<Option<CpuStorageReservation>, CpuError> {
        let Some(budget) = self.budget.filter(|_| count != 0) else {
            return Ok(None);
        };
        let needed = self
            .values
            .len()
            .checked_add(count)
            .ok_or(CpuError::StorageSizeOverflow)?;
        let capacity = if needed <= self.values.capacity() {
            needed
        } else {
            needed
                .checked_next_power_of_two()
                .ok_or(CpuError::StorageSizeOverflow)?
        };
        let clocks = bones
            .checked_mul(size_of::<(u16, M2AnimationClock)>())
            .and_then(|bytes| bytes.checked_mul(count))
            .ok_or(CpuError::StorageSizeOverflow)?;
        let mut plan = CpuStorageWorkingSet::default();
        plan.include(
            self.values
                .reservation_bytes(budget, Class::Frame, capacity)?,
            self.values.replacement_credit(capacity),
        )?;
        plan.include(clocks, 0)?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.values
            .reserve_reserved(&mut fund, Kind::Result, capacity)?;
        Ok(Some(fund))
    }

    pub(super) fn primary(
        &mut self,
        clock: M2AnimationClock,
        event_window: M2EventTimeWindow,
    ) -> Result<(), CpuError> {
        let Some(_fund) = self.reserve_group(1, 0)? else {
            return Ok(());
        };
        self.values.push(M2ExpiredVariation {
            clock,
            event_window,
            bone_sequences: CpuBuffer::default(),
        })
    }

    /// Freeze every queued event before any tied completion can mutate the playback.
    pub(super) fn group(
        &mut self,
        clock: M2AnimationClock,
        bones: &[(u16, M2AnimationClock)],
        queue: &[M2QueuedCallback],
    ) -> Result<(), CpuError> {
        let count = queue.iter().filter(|queued| queued.event.is_some()).count();
        let Some(mut fund) = self.reserve_group(count, bones.len())? else {
            return Ok(());
        };
        for queued in queue {
            let Some(index) = queued.event else {
                continue;
            };
            let mut clocks = CpuBuffer::default();
            clocks.reserve_reserved(&mut fund, Kind::Result, bones.len())?;
            clocks.extend_from_slice(bones)?;
            self.values.push(M2ExpiredVariation {
                clock,
                event_window: M2EventTimeWindow::queued_event(queued.slot.sequence, index),
                bone_sequences: clocks,
            })?;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> CpuBuffer<M2ExpiredVariation> {
        self.values
    }
}
