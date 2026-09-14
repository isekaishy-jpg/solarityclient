//! Retained requests follow actual callbacks and active attachment consumers.

use solarity_asset::DecodedM2Model;
use solarity_rendering::{M2EventTimeWindow, triggered_m2_event_indices};

use super::super::super::{M2GpuPlacementOwner, placement_owner_guid, sound};

/// The current CPU transaction's requested bones; sampling closes their ancestry.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct CpuBoneDemand {
    bones: Vec<usize>,
}

impl CpuBoneDemand {
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        self.bones.clear();
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
            self.bones.push(index);
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
