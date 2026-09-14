//! Owned sampling inputs cross the worker boundary; publication stays on the client.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2BonePoseError, M2BonePoseOverrides, M2FingerPoseHands,
};
use std::sync::Arc;

/// Reusable current-frame sampling inputs and a single-consumer palette.
pub(super) struct PoseJob {
    placement: usize,
    model: Arc<DecodedM2Model>,
    orientation: Vec<bool>,
    clock: M2AnimationClock,
    view: Mat4,
    fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
    transforms: Vec<(u16, Mat4)>,
    sequences: Vec<(u16, M2AnimationClock)>,
    pose: M2BonePose,
    result: Option<Result<(), M2BonePoseError>>,
}

impl PoseJob {
    /// Pins the decoded generation while retaining scratch storage for later frames.
    pub(super) fn new(model: Arc<DecodedM2Model>) -> Self {
        Self {
            placement: 0,
            model,
            orientation: Vec::new(),
            clock: M2AnimationClock::new(0, 0., 0.),
            view: Mat4::IDENTITY,
            fingers: None,
            transforms: Vec::new(),
            sequences: Vec::new(),
            pose: M2BonePose::default(),
            result: None,
        }
    }

    pub(super) fn placement(&self) -> usize {
        self.placement
    }

    /// Replaces all frame inputs; no old sampling result survives preparation.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        placement: usize,
        source: &M2GpuSource,
        clock: M2AnimationClock,
        view: Mat4,
        fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
        transforms: &[(u16, Mat4)],
        sequences: Vec<(u16, M2AnimationClock)>,
    ) {
        self.placement = placement;
        if !Arc::ptr_eq(&self.model, &source.model) {
            self.model = Arc::clone(&source.model);
        }
        self.orientation
            .clone_from(&source.model_oriented_billboard_bones);
        self.clock = clock;
        self.view = view;
        self.fingers = fingers;
        self.transforms.clear();
        self.transforms.extend_from_slice(transforms);
        self.sequences = sequences;
        self.result = None;
    }

    /// Computes pure skeletal work without publishing errors or scene state.
    pub(super) fn sample(&mut self) {
        self.result = Some(self.pose.recompose_with_overrides(
            self.model.animations(),
            self.clock,
            self.view,
            M2BonePoseOverrides {
                model_oriented_billboard_bones: &self.orientation,
                finger_pose: self.fingers,
                bone_transforms: &self.transforms,
                bone_sequences: &self.sequences,
            },
        ));
    }

    /// Publishes a matching palette or leaves the ordinary consumer responsible.
    pub(super) fn take(
        &mut self,
        model: &Arc<DecodedM2Model>,
        clock: M2AnimationClock,
        view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        // Attachments or later owner changes can alter a preselected input.
        // Exact validity includes the camera and every semantic override; a
        // stale palette is never published merely because its owner is unchanged.
        if !Arc::ptr_eq(&self.model, model)
            || self.clock != clock
            || self.view != view
            || self.fingers != overrides.finger_pose
            || self.orientation != overrides.model_oriented_billboard_bones
            || self.transforms != overrides.bone_transforms
            || self.sequences != overrides.bone_sequences
        {
            return Ok(false);
        }
        let Some(result) = self.result.take() else {
            return Ok(false);
        };
        result?;
        std::mem::swap(&mut self.pose, output);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/unit_pose_batch.rs"]
mod tests;
