//! Actual mesh and effect consumers select spatial lighting and ancestor work.

mod frame;

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_receiver_demand.rs"]
mod tests;

use glam::Vec3;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageWorkingSet,
};

use super::super::RuntimeTerrainFrameError;

/// Ancestry inputs survive until all visibility and alpha decisions are known.
#[derive(Clone, Copy)]
struct ReceiverRequest {
    parent: Option<usize>,
    center: Vec3,
    fog: Option<Vec3>,
}

/// Frame-local sparse requests; no historical pose or lighting result is cached.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct ReceiverFrame {
    requests: CpuBuffer<Option<ReceiverRequest>>,
    touched: CpuBuffer<usize>,
    needed: CpuBuffer<bool>,
    remap: CpuBuffer<Option<u32>>,
}

impl ReceiverFrame {
    /// Reserve complete receiver bookkeeping before callbacks can record a scene.
    pub(in crate::application::terrain_frame::m2) fn prepare_storage(
        &mut self,
        budget: &CpuStorageBudget,
        maximum: usize,
    ) -> Result<(), CpuError> {
        let maximum = maximum.max(self.requests.len());
        let mut plan = CpuStorageWorkingSet::default();
        plan.include(
            self.requests
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.requests.replacement_credit(maximum),
        )?;
        plan.include(
            self.touched
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.touched.replacement_credit(maximum),
        )?;
        plan.include(
            self.needed
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.needed.replacement_credit(maximum),
        )?;
        plan.include(
            self.remap
                .reservation_bytes(budget, Class::Frame, maximum)?,
            self.remap.replacement_credit(maximum),
        )?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.requests
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        self.touched
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        self.needed
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        self.remap
            .reserve_reserved(&mut fund, Kind::Metadata, maximum)?;
        self.requests.resize_with(maximum, || None)?;
        self.needed.resize_with(maximum, || false)?;
        self.remap.resize_with(maximum, || None)?;
        Ok(())
    }

    /// Every submitted packet must retain its selected receiver; losing it is
    /// an orchestration error, never permission to use a different light bank.
    fn resolved(&self, placement: u32) -> Result<u32, RuntimeTerrainFrameError> {
        self.remap
            .get(placement as usize)
            .copied()
            .flatten()
            .ok_or(RuntimeTerrainFrameError::MissingM2Receiver { placement })
    }

    /// Clears only records used by the preceding frame, including retired indices.
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        for index in self.touched.drain() {
            self.requests[index] = None;
            self.needed[index] = false;
            self.remap[index] = None;
        }
    }

    /// Records dependency facts without running a spatial callback or making a scene.
    pub(in crate::application::terrain_frame::m2) fn record(
        &mut self,
        budget: &CpuStorageBudget,
        placement: usize,
        parent: Option<usize>,
        center: Vec3,
        fog: Option<Vec3>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let index = u32::try_from(placement)
            .map_err(|_| solarity_rendering::VulkanError::WorldFrameCapacity)?;
        if placement >= self.requests.len() {
            let end = placement
                .checked_add(1)
                .and_then(usize::checked_next_power_of_two)
                .ok_or(CpuError::StorageSizeOverflow)?;
            self.prepare_storage(budget, end)?;
        }
        if self.requests[placement].is_none() {
            self.touched.push(placement)?;
        }
        self.requests[placement] = Some(ReceiverRequest {
            parent,
            center,
            fog,
        });
        Ok(index)
    }

    /// Fog selection follows the admitted model's liquid and portal classification.
    pub(in crate::application::terrain_frame::m2) fn set_fog(
        &mut self,
        placement: usize,
        fog: Option<Vec3>,
    ) {
        if let Some(Some(request)) = self.requests.get_mut(placement) {
            request.fog = fog;
        }
    }

    /// A rendered child requires the callback and center of every retained ancestor.
    fn require(&mut self, placement: u32) {
        let mut current = placement as usize;
        while let Some(Some(request)) = self.requests.get(current) {
            if self.needed[current] {
                break;
            }
            self.needed[current] = true;
            let Some(parent) = request.parent else { break };
            current = parent;
        }
    }
}
