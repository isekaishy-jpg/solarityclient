//! Owned sampling inputs cross the worker boundary; publication stays on the client.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_asset::ResourceLease;
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2BonePoseError, M2BonePoseOverrides, M2BoneSamples,
    M2FingerPoseHands,
};

/// Reusable current-frame sampling inputs and a single-consumer palette.
pub(super) struct PoseJob {
    pub(super) measurement: solarity_cpu::WorkMeasurement,
    placement: usize,
    trace: solarity_profiling::TraceContext,
    model: Option<ResourceLease<DecodedM2Model>>,
    orientation: Vec<bool>,
    clock: M2AnimationClock,
    view: Mat4,
    fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
    transforms: Vec<(u16, Mat4)>,
    sequences: Vec<(u16, M2AnimationClock)>,
    pose: M2BonePose,
    samples: M2BoneSamples,
    requested: solarity_cpu::CpuBuffer<usize>,
    sparse: bool,
    result: Option<Result<(), M2BonePoseError>>,
}

impl PoseJob {
    /// Skeletal output and scratch are charged before any worker receives input.
    pub(super) fn admit(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), solarity_cpu::CpuError> {
        if self.sparse {
            self.samples.reserve_cpu_storage(
                cpu.storage(),
                self.model
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("prepared pose retains model"))
                    .animations()
                    .bones()
                    .len(),
            )
        } else {
            self.pose.reserve_cpu_storage(
                cpu.storage(),
                self.model
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("prepared pose retains model"))
                    .animations()
                    .bones()
                    .len(),
            )
        }
    }

    /// Offscreen CPU consumers retain only their named demand across the worker turn.
    pub(super) fn request_samples(
        &mut self,
        bones: &[usize],
        budget: &solarity_cpu::CpuStorageBudget,
    ) -> Result<(), solarity_cpu::CpuError> {
        self.requested.clear();
        self.requested.reserve(
            budget,
            solarity_cpu::CpuStorageClass::Frame,
            solarity_cpu::CpuStorageKind::Metadata,
            bones.len(),
        )?;
        self.requested.extend_from_slice(bones)?;
        self.sparse = true;
        Ok(())
    }

    /// Required frame sampling completes even after consumer withdrawal; errors
    /// remain attached to the exact model and are observed in traversal order.
    pub(super) fn execute(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        context.diagnostic_value(
            "m2.pose.bones",
            self.model
                .as_ref()
                .unwrap_or_else(|| unreachable!("prepared pose retains model"))
                .animations()
                .bones()
                .len() as u64,
        );
        context.diagnostic_value("m2.pose.named_only", u64::from(self.sparse));
        self.sample();
        solarity_cpu::JobOutcome::Succeeded
    }

    /// A result remains present when traversal rejected it or never requested it.
    pub(super) fn unconsumed(&self) -> bool {
        self.result.is_some()
    }

    /// Pins the decoded generation while retaining scratch storage for later frames.
    pub(super) fn new(model: ResourceLease<DecodedM2Model>) -> Self {
        Self {
            measurement: solarity_cpu::WorkMeasurement::default(),
            placement: 0,
            trace: solarity_profiling::TraceContext::default(),
            model: Some(model),
            orientation: Vec::new(),
            clock: M2AnimationClock::new(0, 0., 0.),
            view: Mat4::IDENTITY,
            fingers: None,
            transforms: Vec::new(),
            sequences: Vec::new(),
            pose: M2BonePose::default(),
            samples: M2BoneSamples::default(),
            requested: solarity_cpu::CpuBuffer::default(),
            sparse: false,
            result: None,
        }
    }

    pub(super) fn release_model(&mut self) {
        self.model = None;
        self.result = None;
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
        if self
            .model
            .as_ref()
            .is_none_or(|model| !ResourceLease::ptr_eq(model, &source.model))
        {
            self.model = Some(ResourceLease::clone(&source.model));
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
        self.sparse = false;
        self.requested.clear();
    }

    /// Computes pure skeletal work without publishing errors or scene state.
    pub(super) fn sample(&mut self) {
        let started = self.measurement.start();
        self.trace.link("m2.pose.execute");
        let _trace = self.trace.enter();
        let mut _profile_scope = solarity_profiling::detail_profile!(
            "runtime.application.terrain_frame.m2.preparation.poses.input.sample"
        );
        _profile_scope.trace_owner(self.placement as u64 + 1, 0);
        let overrides = M2BonePoseOverrides {
            model_oriented_billboard_bones: &self.orientation,
            finger_pose: self.fingers,
            bone_transforms: &self.transforms,
            bone_sequences: &self.sequences,
        };
        self.result = Some(if self.sparse {
            self.samples.recompose(
                self.model
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("prepared pose retains model"))
                    .animations(),
                self.clock,
                self.view,
                overrides,
                &self.requested,
            )
        } else {
            self.pose.recompose_with_overrides(
                self.model
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("prepared pose retains model"))
                    .animations(),
                self.clock,
                self.view,
                overrides,
            )
        });
        if self.result.as_ref().is_some_and(Result::is_ok) {
            self.measurement.finish(started);
        }
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
        if self.sparse || !self.matches(model, clock, view, overrides) {
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

    /// Sparse output cannot become a render palette or satisfy changed callback demand.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn take_samples(
        &mut self,
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
        output: &mut M2BoneSamples,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if !self.sparse || &*self.requested != bones || !self.matches(model, clock, view, overrides)
        {
            return Ok(false);
        }
        let Some(result) = self.result.take() else {
            return Ok(false);
        };
        result?;
        self.trace.link("m2.pose.consume_samples");
        std::mem::swap(&mut self.samples, output);
        Ok(true)
    }

    fn matches(
        &self,
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
    ) -> bool {
        self.model
            .as_ref()
            .is_some_and(|selected| ResourceLease::ptr_eq(selected, model))
            && self.clock == clock
            && self.view == view
            && self.fingers == overrides.finger_pose
            && self.orientation == overrides.model_oriented_billboard_bones
            && self.transforms == overrides.bone_transforms
            && self.sequences == overrides.bone_sequences
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/unit_pose_batch.rs"]
mod tests;
