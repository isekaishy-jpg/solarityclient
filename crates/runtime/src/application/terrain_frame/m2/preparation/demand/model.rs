//! Current model consumers determine CPU work after final placement admission.

use super::super::super::attachments::{ItemRequests, VisualRequests};
use solarity_asset::DecodedM2Model;
use solarity_rendering::M2EventTimeWindow;

use super::super::super::{M2GpuPlacement, M2GpuPlacementOwner, unit_effects::M2UnitEffectScene};
use super::CpuBoneDemand;

/// Borrowed scene ownership facts; this view never advances a timer or callback.
pub(in crate::application::terrain_frame::m2) struct CpuModelInputs<'a> {
    pub placement: &'a M2GpuPlacement,
    pub model: &'a DecodedM2Model,
    pub window: M2EventTimeWindow,
    pub items: &'a ItemRequests,
    pub visuals: &'a VisualRequests,
    pub glue_ids: &'a [u32],
    pub effects: &'a M2UnitEffectScene,
    pub publishes_lights: bool,
}

impl CpuBoneDemand {
    /// A callback-only unit with no active consumers requests no skeleton work.
    pub(in crate::application::terrain_frame::m2) fn model(&mut self, input: CpuModelInputs<'_>) {
        let CpuModelInputs {
            placement,
            model,
            window,
            items,
            visuals,
            glue_ids,
            effects,
            publishes_lights,
        } = input;
        self.clear();
        self.events(model, placement.owner, window);
        if let Some(retired) = &placement.retirement {
            for &id in retired.attachments() {
                self.attachment(model, id);
            }
        }
        if let Some(animation) = &placement.unit_animation {
            effects.request_anchor_bones(animation, model, self);
        }
        if publishes_lights || matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }) {
            for light in model.animations().lights() {
                if let Some(index) = light.bone_index() {
                    self.bone(model, usize::from(index));
                }
            }
        }
        match placement.owner {
            M2GpuPlacementOwner::GlueModel { .. } => {
                for &id in glue_ids {
                    self.attachment(model, id);
                }
            }
            M2GpuPlacementOwner::PlayerMount { .. } => {
                self.mount_camera(model);
                self.attachment(model, 0);
            }
            M2GpuPlacementOwner::RemotePlayerMount { .. }
            | M2GpuPlacementOwner::CreatureMount { .. } => self.attachment(model, 0),
            M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } => {
                for &(_, point) in items.for_owner(guid) {
                    self.attachment(model, point.id());
                }
            }
            M2GpuPlacementOwner::UnitItem { guid, point } => {
                for &(_, _, effect) in visuals.for_owner((guid, point)) {
                    self.attachment(model, effect);
                }
            }
            M2GpuPlacementOwner::Retired(_)
            | M2GpuPlacementOwner::UnitEffect { .. }
            | M2GpuPlacementOwner::Static(_)
            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            | M2GpuPlacementOwner::GluePet
            | M2GpuPlacementOwner::GameObject { .. }
            | M2GpuPlacementOwner::UnitItemVisual { .. } => {}
        }
    }
}
