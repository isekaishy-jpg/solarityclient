//! Owned sampling inputs cross the worker boundary; publication stays on the client.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_asset::ResourceLease;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
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
    layout: solarity_asset::ResourceWeak<DecodedM2Model>,
    orientation: CpuBuffer<bool>,
    clock: M2AnimationClock,
    view: Mat4,
    fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
    transforms: CpuBuffer<(u16, Mat4)>,
    sequences: CpuBuffer<(u16, M2AnimationClock)>,
    pose: M2BonePose,
    samples: M2BoneSamples,
    requested: solarity_cpu::CpuBuffer<usize>,
    sparse: bool,
    result: Option<Result<(), M2BonePoseError>>,
}

impl PoseJob {
    /// A single discovered dependency uses the same reservation as batch output admission.
    #[cfg(test)]
    pub(super) fn admit(&mut self, cpu: &solarity_cpu::CpuExecutor) -> Result<(), CpuError> {
        let mut plan = CpuStorageWorkingSet::default();
        self.include_output(cpu.storage(), &mut plan)?;
        let mut fund = cpu
            .storage()
            .reserve_working_set(Class::Frame, plan.bytes())?;
        self.admit_output(&mut fund)
    }

    pub(super) fn include_output(
        &self,
        budget: &CpuStorageBudget,
        plan: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        let bones = self
            .model
            .as_ref()
            .unwrap_or_else(|| unreachable!("prepared pose retains model"))
            .animations()
            .bones()
            .len();
        if self.sparse {
            self.samples.include_cpu_storage(budget, bones, plan)
        } else {
            self.pose.include_cpu_storage(budget, bones, plan)
        }
    }

    pub(super) fn admit_output(
        &mut self,
        fund: &mut CpuStorageReservation,
    ) -> Result<(), CpuError> {
        let bones = self
            .model
            .as_ref()
            .unwrap_or_else(|| unreachable!("prepared pose retains model"))
            .animations()
            .bones()
            .len();
        if self.sparse {
            self.samples.reserve_cpu_storage_reserved(fund, bones)
        } else {
            self.pose.reserve_cpu_storage_reserved(fund, bones)
        }
    }

    /// Complete copied input storage is admitted before replacing any prior input values.
    fn prepare_inputs(
        &mut self,
        budget: &CpuStorageBudget,
        orientation: &[bool],
        transforms: &[(u16, Mat4)],
        sequences: impl Iterator<Item = (u16, M2AnimationClock)>,
    ) -> Result<(), CpuError> {
        let count = sequences
            .size_hint()
            .1
            .ok_or(CpuError::StorageSizeOverflow)?;
        let mut plan = CpuStorageWorkingSet::default();
        plan.include(
            self.orientation
                .reservation_bytes(budget, Class::Frame, orientation.len())?,
            self.orientation.replacement_credit(orientation.len()),
        )?;
        plan.include(
            self.transforms
                .reservation_bytes(budget, Class::Frame, transforms.len())?,
            self.transforms.replacement_credit(transforms.len()),
        )?;
        plan.include(
            self.sequences
                .reservation_bytes(budget, Class::Frame, count)?,
            self.sequences.replacement_credit(count),
        )?;
        let mut fund = budget.reserve_working_set(Class::Frame, plan.bytes())?;
        self.orientation
            .reserve_reserved(&mut fund, Kind::Scratch, orientation.len())?;
        self.transforms
            .reserve_reserved(&mut fund, Kind::Scratch, transforms.len())?;
        self.sequences
            .reserve_reserved(&mut fund, Kind::Scratch, count)?;
        self.orientation.clear();
        self.orientation.extend_from_slice(orientation)?;
        self.transforms.clear();
        self.transforms.extend_from_slice(transforms)?;
        self.sequences.clear();
        for value in sequences {
            self.sequences.push(value)?;
        }
        Ok(())
    }

    /// Offscreen CPU consumers retain only their named demand across the worker turn.
    pub(super) fn request_samples(
        &mut self,
        bones: &[usize],
        budget: &solarity_cpu::CpuStorageBudget,
    ) -> Result<(), solarity_cpu::CpuError> {
        self.requested.reserve(
            budget,
            solarity_cpu::CpuStorageClass::Frame,
            solarity_cpu::CpuStorageKind::Metadata,
            bones.len(),
        )?;
        self.requested.clear();
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
            layout: ResourceLease::downgrade(&model),
            model: Some(model),
            orientation: CpuBuffer::default(),
            clock: M2AnimationClock::new(0, 0., 0.),
            view: Mat4::IDENTITY,
            fingers: None,
            transforms: CpuBuffer::default(),
            sequences: CpuBuffer::default(),
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

    pub(super) fn retains_layout(&self, source: &M2GpuSource) -> bool {
        self.layout.as_ptr() == ResourceLease::as_ptr(&source.model)
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
        sequences: impl Iterator<Item = (u16, M2AnimationClock)>,
        budget: &CpuStorageBudget,
    ) -> Result<(), CpuError> {
        self.prepare_inputs(
            budget,
            &source.model_oriented_billboard_bones,
            transforms,
            sequences,
        )?;
        self.placement = placement;
        self.trace = solarity_profiling::TraceContext::capture().fork("m2.pose.request");
        if !self.retains_layout(source) {
            self.pose = M2BonePose::default();
            self.samples = M2BoneSamples::default();
            self.requested = solarity_cpu::CpuBuffer::default();
            self.layout = ResourceLease::downgrade(&source.model);
        }
        if self
            .model
            .as_ref()
            .is_none_or(|model| !ResourceLease::ptr_eq(model, &source.model))
        {
            self.model = Some(ResourceLease::clone(&source.model));
        }
        self.clock = clock;
        self.view = view;
        self.fingers = fingers;
        self.result = None;
        self.sparse = false;
        self.requested.clear();
        Ok(())
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

    /// Successful supersets may serve another callback without recomputation. A failed
    /// broad request cannot introduce an error for a narrower, otherwise valid consumer.
    pub(super) fn matches_samples(
        &self,
        model: &ResourceLease<DecodedM2Model>,
        clock: M2AnimationClock,
        view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
    ) -> bool {
        self.sparse
            && self.matches(model, clock, view, overrides)
            && self.result.is_some()
            && (&*self.requested == bones
                || (self.result.as_ref().is_some_and(Result::is_ok)
                    && bones.iter().all(|bone| self.requested.contains(bone))))
    }

    pub(super) fn samples(&mut self) -> Result<&M2BoneSamples, RuntimeTerrainFrameError> {
        if self.result.as_ref().is_some_and(Result::is_err) {
            self.result
                .take()
                .unwrap_or_else(|| unreachable!("sample result exists"))?;
        }
        Ok(&self.samples)
    }

    /// The frozen serial reference uses exactly the same inputs without worker admission.
    pub(super) fn sample_unadmitted(&mut self, bones: &[usize]) {
        self.sparse = true;
        self.result = Some(
            self.samples.recompose(
                self.model
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("sample owns its model"))
                    .animations(),
                self.clock,
                self.view,
                M2BonePoseOverrides {
                    model_oriented_billboard_bones: &self.orientation,
                    finger_pose: self.fingers,
                    bone_transforms: &self.transforms,
                    bone_sequences: &self.sequences,
                },
                bones,
            ),
        );
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
            && &*self.orientation == overrides.model_oriented_billboard_bones
            && &*self.transforms == overrides.bone_transforms
            && &*self.sequences == overrides.bone_sequences
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/unit_pose_batch.rs"]
mod tests;
