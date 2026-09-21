//! Ancestor-closed CPU sampling never exposes incomplete storage as a GPU palette.

use glam::Mat4;
use solarity_asset::M2AnimationSet;

use super::super::M2BoneTransforms;
use super::{
    BillboardView, M2AnimationClock, M2BonePose, M2BonePoseError, M2BonePoseOverrides,
    finite_matrix,
};

/// Current samples for CPU attachments, events and light emitters. This type has
/// no palette accessor and cannot be used for an M2 mesh upload.
#[derive(Default)]
pub struct M2BoneSamples {
    scratch: M2BonePose,
    required: Vec<bool>,
    valid: bool,
    /// Released after the ancestor-selection allocation and its pose scratch.
    memory: Option<solarity_cpu::ByteReservation>,
}

impl M2BoneSamples {
    /// Admits ancestor-selection storage and the complete reusable pose scratch.
    /// This changes no selected bones or sampled values.
    ///
    /// # Errors
    /// Reports byte pressure or allocation failure before sampling starts.
    pub fn reserve_cpu_storage(
        &mut self,
        budget: &solarity_cpu::CpuStorageBudget,
        bones: usize,
    ) -> Result<(), solarity_cpu::CpuError> {
        use solarity_cpu::{CpuError, CpuStorageClass, CpuStorageKind, CpuStorageWorkingSet};
        let class = CpuStorageClass::Frame;
        let kind = CpuStorageKind::Scratch;
        let mut working_set = CpuStorageWorkingSet::default();
        self.scratch
            .include_cpu_storage(budget, bones, &mut working_set)?;
        let adoption = self
            .memory
            .as_ref()
            .map_or(self.required.capacity(), |memory| {
                memory.admission_bytes(budget, class)
            });
        let (replacement, retired) = if bones > self.required.capacity() {
            (bones, self.required.capacity())
        } else {
            (0, 0)
        };
        working_set.include(
            adoption
                .checked_add(replacement)
                .ok_or(CpuError::StorageSizeOverflow)?,
            retired,
        )?;
        let mut reservation = budget.reserve_working_set(class, working_set.bytes())?;
        self.scratch
            .reserve_cpu_storage_reserved(&mut reservation, bones)?;
        if let Some(memory) = &mut self.memory {
            memory.transfer_reserved(&mut reservation, kind)?;
        } else {
            self.memory = Some(reservation.reserve(kind, self.required.capacity())?);
        }
        if bones > self.required.capacity() {
            let mut memory = reservation.reserve(kind, bones)?;
            let mut required = Vec::new();
            required
                .try_reserve_exact(bones)
                .map_err(|_| CpuError::StorageAllocation)?;
            required.extend_from_slice(&self.required);
            memory.resize_reserved(&mut reservation, required.capacity())?;
            self.required = required;
            if let Some(retired) = self.memory.replace(memory) {
                reservation.recycle(retired)?;
            }
        }
        Ok(())
    }

    /// Actual retained ancestor marks and nested skeletal working arrays.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.scratch.allocated_bytes() + self.required.capacity()
    }

    /// Samples requested bones and every ancestor using the full-pose arithmetic.
    /// Replaces the complete prior request, including after model replacement.
    ///
    /// # Errors
    /// Returns invalid requested indices or the ordinary pose validation errors.
    pub fn recompose(
        &mut self,
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
        bones: &[usize],
    ) -> Result<(), M2BonePoseError> {
        let _profile_scope = solarity_profiling::detail_profile!(
            "rendering.model.m2_animation.pose.compose.samples.recompose"
        );
        self.valid = false;
        if self.memory.is_some() && animations.bones().len() > self.required.capacity() {
            return Err(M2BonePoseError::StorageCapacity {
                requested: animations.bones().len(),
                available: self.required.capacity(),
            });
        }
        self.required.resize(animations.bones().len(), false);
        self.required.fill(false);
        for &bone in bones {
            if bone >= self.required.len() {
                return Err(M2BonePoseError::RequestedBoneIndex {
                    requested: bone,
                    available: self.required.len(),
                });
            }
            let mut next = Some(bone);
            while let Some(index) = next {
                if self.required[index] {
                    break;
                }
                self.required[index] = true;
                next = animations.bones()[index].parent().map(usize::from);
            }
        }
        let determinant = model_view.determinant();
        if !finite_matrix(model_view) || !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
            return Err(M2BonePoseError::InvalidModelView);
        }
        self.scratch.recompose_inner(
            animations,
            clock,
            Some(BillboardView {
                model_view,
                inverse_model_view: model_view.inverse(),
            }),
            overrides.model_oriented_billboard_bones,
            overrides.finger_pose,
            overrides.bone_transforms,
            overrides.bone_sequences,
            Some(&self.required),
        )?;
        self.valid = true;
        Ok(())
    }
}

impl M2BoneTransforms for M2BoneSamples {
    fn bone_count(&self) -> usize {
        self.required.len()
    }
    fn bone_transform(&self, index: usize) -> Option<Mat4> {
        (self.valid && self.required.get(index).copied().unwrap_or(false))
            .then(|| self.scratch.transforms.get(index).copied())
            .flatten()
    }
}
