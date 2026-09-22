//! Retained requests follow actual callbacks and active attachment consumers.

use solarity_asset::DecodedM2Model;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use solarity_rendering::{M2EventTimeWindow, triggered_m2_event_indices};

use super::super::super::{M2GpuPlacementOwner, placement_owner_guid, sound};

/// The current CPU transaction's requested bones; sampling closes their ancestry.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct CpuBoneDemand {
    bones: CpuBuffer<usize>,
    seen: CpuBuffer<u64>,
    limit: usize,
}

impl CpuBoneDemand {
    pub(in crate::application::terrain_frame::m2) fn include_storage(
        &self,
        budget: &CpuStorageBudget,
        bones: usize,
        plan: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        plan.include(
            self.bones.reservation_bytes(budget, Class::Frame, bones)?,
            self.bones.replacement_credit(bones),
        )?;
        let words = bones.div_ceil(64);
        plan.include(
            self.seen.reservation_bytes(budget, Class::Frame, words)?,
            self.seen.replacement_credit(words),
        )
    }

    pub(in crate::application::terrain_frame::m2) fn reserve_reserved(
        &mut self,
        fund: &mut CpuStorageReservation,
        bones: usize,
    ) -> Result<(), CpuError> {
        self.bones.reserve_reserved(fund, Kind::Scratch, bones)?;
        self.seen
            .reserve_reserved(fund, Kind::Scratch, bones.div_ceil(64))
    }

    /// One entry per authored bone bounds all event/attachment/light aliases.
    /// A failed reservation preserves the preceding demand and its backing storage.
    pub(in crate::application::terrain_frame::m2) fn begin(
        &mut self,
        budget: &CpuStorageBudget,
        bones: usize,
    ) -> Result<(), CpuError> {
        let mut plan = CpuStorageWorkingSet::default();
        self.include_storage(budget, bones, &mut plan)?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.reserve_reserved(&mut fund, bones)?;
        self.bones.clear();
        self.seen.resize_with(bones.div_ceil(64), || 0)?;
        self.seen.fill(0);
        self.limit = bones;
        Ok(())
    }

    pub(in crate::application::terrain_frame::m2) fn bones(&self) -> &[usize] {
        &self.bones
    }

    /// Consumers retain their existing errors for malformed references. Unused
    /// or disabled declarations must not acquire eager sampling failures.
    pub(in crate::application::terrain_frame::m2) fn bone(
        &mut self,
        model: &DecodedM2Model,
        index: usize,
    ) {
        if index < model.animations().bones().len() {
            self.insert(index);
        }
    }

    fn insert(&mut self, index: usize) {
        assert!(
            index < self.limit,
            "bone demand begins with its complete model bound"
        );
        let mask = 1_u64 << (index % 64);
        let word = &mut self.seen[index / 64];
        if *word & mask == 0 {
            *word |= mask;
            self.bones
                .push(index)
                .unwrap_or_else(|_| unreachable!("unique bone fits its admitted source bound"));
        }
    }

    /// 831330 queries only requested attachment IDs, not the whole model table.
    pub(in crate::application::terrain_frame::m2) fn attachment(
        &mut self,
        model: &DecodedM2Model,
        id: u32,
    ) {
        if let Some(attachment) = model.attachment(id) {
            self.bone(model, usize::from(attachment.bone_index()));
        }
    }

    /// Event positions are needed only for declarations crossing this interval.
    pub(in crate::application::terrain_frame::m2) fn events(
        &mut self,
        model: &DecodedM2Model,
        owner: M2GpuPlacementOwner,
        window: M2EventTimeWindow,
    ) {
        for index in triggered_m2_event_indices(model.animations(), window) {
            let event = &model.animations().events()[index];
            if let Some(bone) = event.bone_index() {
                self.bone(model, bone as usize);
            }
            if event.identifier() == *b"$CSD"
                && sound::M2SoundKind::for_placement(owner).is_none()
                && placement_owner_guid(owner).is_some()
            {
                self.attachment(model, 17);
            }
        }
    }

    /// The active mount camera reads its authored $CMA bone without event timing.
    pub(super) fn mount_camera(&mut self, model: &DecodedM2Model) {
        if let Some(bone) = model
            .animations()
            .events()
            .iter()
            .find(|event| event.identifier() == *b"$CMA")
            .and_then(|event| event.bone_index())
        {
            self.bone(model, bone as usize);
        }
    }
}

/// A complete model domain with no requested CPU transforms. GPU palette jobs
/// own validation/composition; ordered callbacks cannot observe stale scratch.
pub(in crate::application::terrain_frame::m2) struct UnrequestedBones(pub usize);

impl solarity_rendering::M2BoneTransforms for UnrequestedBones {
    fn bone_count(&self) -> usize {
        self.0
    }
    fn bone_transform(&self, _index: usize) -> Option<glam::Mat4> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_demand_preserves_first_request_order_and_refuses_before_reset()
    -> Result<(), CpuError> {
        let bytes = 130 * size_of::<usize>() + 3 * size_of::<u64>();
        let denied = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes - 1, 0, 0));
        let mut demand = CpuBoneDemand::default();
        assert!(demand.begin(&denied, 130).is_err());
        assert_eq!(demand.bones.capacity(), 0);
        assert_eq!(demand.seen.capacity(), 0);
        assert_eq!(denied.snapshot().used(Class::Frame), 0);
        let budget = CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(bytes, 0, 0));
        demand.begin(&budget, 130)?;
        for index in [129, 0, 64, 129, 0, 63] {
            demand.insert(index);
        }
        assert_eq!(demand.bones(), &[129, 0, 64, 63]);
        assert!(demand.begin(&budget, 131).is_err());
        assert_eq!(demand.bones(), &[129, 0, 64, 63]);
        assert!(demand.begin(&denied, 130).is_err());
        assert_eq!(demand.bones(), &[129, 0, 64, 63]);
        let addresses = (demand.bones.as_ptr(), demand.seen.as_ptr());
        for _ in 0..100 {
            demand.begin(&budget, 130)?;
            for index in (0..130).rev().chain(0..130) {
                demand.insert(index);
            }
            assert_eq!(demand.bones.len(), 130);
            assert_eq!(demand.bones[0], 129);
            assert_eq!(demand.bones[129], 0);
        }
        assert_eq!(addresses, (demand.bones.as_ptr(), demand.seen.as_ptr()));
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        drop(demand);
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
