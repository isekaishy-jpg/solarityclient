//! Parent-first M2 bone transform composition.

use glam::{Mat3, Mat4, Vec3};
use solarity_asset::M2AnimationSet;

use super::M2BonePoseError;
use super::sample::{sample_quaternion, sample_vec3};

/// The local animation and process-global clocks used by every bone track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2AnimationClock {
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
}

impl M2AnimationClock {
    /// Creates one clock snapshot; finiteness is checked when composing a pose.
    #[must_use]
    pub const fn new(sequence: usize, animation_time_ms: f32, global_time_ms: f32) -> Self {
        Self {
            sequence,
            animation_time_ms,
            global_time_ms,
        }
    }
}

/// One complete model-bone matrix palette ready for GPU upload.
#[derive(Clone, Debug, PartialEq)]
pub struct M2BonePose {
    transforms: Vec<Mat4>,
}

impl M2BonePose {
    /// Samples and composes every non-billboard bone in parent order.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] for an invalid/unavailable sequence,
    /// non-finite clock, or a billboard that requires the camera-aware path.
    pub fn compose(
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
    ) -> Result<Self, M2BonePoseError> {
        if !clock.animation_time_ms.is_finite() || !clock.global_time_ms.is_finite() {
            return Err(M2BonePoseError::NonFiniteTime);
        }
        if clock.sequence >= animations.sequences().len() {
            return Err(M2BonePoseError::SequenceIndex {
                requested: clock.sequence,
                available: animations.sequences().len(),
            });
        }
        if animations.is_sequence_available(clock.sequence) != Some(true) {
            return Err(M2BonePoseError::SequenceUnavailable {
                sequence: clock.sequence,
            });
        }
        let sequence = animations.resolve_sequence_alias(clock.sequence).ok_or(
            M2BonePoseError::SequenceIndex {
                requested: clock.sequence,
                available: animations.sequences().len(),
            },
        )?;
        if let Some((bone, _)) = animations
            .bones()
            .iter()
            .enumerate()
            .find(|(_index, bone)| bone.flags() & 0x78 != 0)
        {
            return Err(M2BonePoseError::BillboardViewRequired { bone });
        }

        let mut local = Vec::with_capacity(animations.bones().len());
        for bone in animations.bones() {
            let translation = sample_vec3(
                animations,
                bone.translation(),
                sequence,
                clock.animation_time_ms,
                clock.global_time_ms,
                Vec3::ZERO,
            );
            let rotation = sample_quaternion(
                animations,
                bone.rotation(),
                sequence,
                clock.animation_time_ms,
                clock.global_time_ms,
            );
            let scale = sample_vec3(
                animations,
                bone.scale(),
                sequence,
                clock.animation_time_ms,
                clock.global_time_ms,
                Vec3::ONE,
            );
            local.push(
                Mat4::from_translation(bone.pivot())
                    * Mat4::from_translation(translation)
                    * Mat4::from_quat(rotation)
                    * Mat4::from_scale(scale)
                    * Mat4::from_translation(-bone.pivot()),
            );
        }

        let mut transforms = vec![Mat4::IDENTITY; local.len()];
        let mut states = vec![0_u8; local.len()];
        for index in 0..local.len() {
            compose_bone(index, animations, &local, &mut transforms, &mut states);
        }
        Ok(Self { transforms })
    }

    /// Returns model-space transforms indexed exactly like the M2 bone array.
    #[must_use]
    pub fn transforms(&self) -> &[Mat4] {
        &self.transforms
    }
}

/// Resolves an arbitrarily ordered but cycle-free hierarchy once per bone.
fn compose_bone(
    index: usize,
    animations: &M2AnimationSet,
    local: &[Mat4],
    transforms: &mut [Mat4],
    states: &mut [u8],
) {
    if states[index] == 2 {
        return;
    }
    states[index] = 1;
    if let Some(parent) = animations.bones()[index].parent().map(usize::from) {
        if states[parent] == 0 {
            compose_bone(parent, animations, local, transforms, states);
        }
        transforms[index] =
            inherited_parent_transform(transforms[parent], animations.bones()[index].flags())
                * local[index];
    } else {
        transforms[index] = local[index];
    }
    states[index] = 2;
}

/// Removes parent translation, scale, or rotation selected by low bone flags.
fn inherited_parent_transform(parent: Mat4, flags: u32) -> Mat4 {
    let keep_translation = flags & 0x1 == 0;
    let keep_scale = flags & 0x2 == 0;
    let keep_rotation = flags & 0x4 == 0;
    if keep_translation && keep_scale && keep_rotation {
        return parent;
    }

    let mut scale = Vec3::ONE;
    let mut rotation = Mat3::IDENTITY;
    for column in 0..3 {
        let axis = parent.col(column).truncate();
        let length = axis.length();
        if length.is_finite() && length > 0.000_001 {
            scale[column] = length;
            *rotation.col_mut(column) = axis / length;
        }
    }
    if rotation.determinant() < 0.0 {
        scale.x = -scale.x;
        *rotation.col_mut(0) = -rotation.col(0);
    }
    let mut result = Mat4::IDENTITY;
    if keep_rotation {
        result = Mat4::from_mat3(rotation);
    }
    if keep_scale {
        result *= Mat4::from_scale(scale);
    }
    if keep_translation {
        result.w_axis = parent.w_axis;
    }
    result
}
