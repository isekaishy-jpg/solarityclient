//! Owned sampling inputs cross the worker boundary; publication stays on the client.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_asset::ResourceLease;
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2BonePoseError, M2BonePoseOverrides, M2FingerPoseHands,
};

/// Reusable current-frame sampling inputs and a single-consumer palette.
pub(super) struct PoseJob {
    placement: usize,
    trace: solarity_profiling::TraceContext,
    model: ResourceLease<DecodedM2Model>,
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
    /// A result remains present when traversal rejected it or never requested it.
    pub(super) fn unconsumed(&self) -> bool {
        self.result.is_some()
    }

    /// Pins the decoded generation while retaining scratch storage for later frames.
    pub(super) fn new(model: ResourceLease<DecodedM2Model>) -> Self {
        Self {
            placement: 0,
            trace: solarity_profiling::TraceContext::default(),
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
        self.trace = solarity_profiling::TraceContext::capture().fork("m2.pose.request");
        if !ResourceLease::ptr_eq(&self.model, &source.model) {
            self.model = ResourceLease::clone(&source.model);
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
        self.trace.link("m2.pose.execute");
        let _trace = self.trace.enter();
        let mut _profile_scope = solarity_profiling::detail_profile!(
            "runtime.application.terrain_frame.m2.preparation.poses.input.sample"
        );
        _profile_scope.trace_owner(self.placement as u64 + 1, 0);
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
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        // Attachments or later owner changes can alter a preselected input.
        // Exact validity includes the camera and every semantic override; a
        // stale palette is never published merely because its owner is unchanged.
        if !ResourceLease::ptr_eq(&self.model, model)
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
        self.trace.link("m2.pose.consume");
        std::mem::swap(&mut self.pose, output);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/unit_pose_batch.rs"]
mod tests;
