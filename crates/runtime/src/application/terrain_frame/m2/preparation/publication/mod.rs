//! Ordered CPU publication consumes bone lookups without a render-palette capability.

mod frame;

use super::super::{
    RuntimeM2Event, RuntimeMountCameraSample, retirement::M2RetirementScene,
    scene_lighting::SceneLighting, unit_effects::M2UnitEffectScene,
};
use glam::Mat4;
use solarity_rendering::{
    CharacterAttachmentPoint, M2AnimationClock, M2DirectionalLight, M2EventTimeWindow, M2PointLight,
};

/// Borrowed destinations preserve callback and attachment publication order.
pub(super) struct CpuPublication<'a> {
    pub(super) retirement: &'a mut M2RetirementScene,
    pub(super) triggered_events: &'a mut Vec<RuntimeM2Event>,
    pub(super) unit_effects: &'a mut M2UnitEffectScene,
    pub(super) scene_lighting: &'a mut SceneLighting,
    pub(super) glue_directional_lights: &'a mut Vec<M2DirectionalLight>,
    pub(super) glue_point_lights: &'a mut Vec<M2PointLight>,
    pub(super) glue_attachment_ids: &'a [u32],
    pub(super) glue_attachment_transforms: &'a mut Vec<(u32, Option<Mat4>)>,
    pub(super) mount_camera_sample: &'a mut Option<RuntimeMountCameraSample>,
    pub(super) rider_transforms: &'a mut Vec<(u64, Option<Mat4>)>,
    pub(super) requested_items: &'a [(u64, CharacterAttachmentPoint)],
    pub(super) item_transforms: &'a mut Vec<(u64, CharacterAttachmentPoint, Option<Mat4>)>,
    pub(super) requested_visuals: &'a [(u64, CharacterAttachmentPoint, u32)],
    pub(super) visual_transforms: &'a mut Vec<(u64, CharacterAttachmentPoint, u32, Option<Mat4>)>,
}

/// Clocks and visibility-independent state selected before CPU publication.
pub(super) struct CpuSample {
    pub(super) clock: M2AnimationClock,
    pub(super) event_window: M2EventTimeWindow,
    pub(super) animation_time_ms: f32,
    pub(super) placement_opacity: f32,
    pub(super) publishes_lights: bool,
}
