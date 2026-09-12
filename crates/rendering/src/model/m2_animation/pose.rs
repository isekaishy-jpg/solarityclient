//! Parent-first M2 bone transform composition.

use glam::{Mat3, Mat4, Vec3};
use solarity_asset::{M2AnimationSet, M2Attachment, M2ParticleEmitter, M2Track};

use super::quaternion::quaternion_matrix;
use super::sample::sample_discrete;
use super::sample::{sample_quaternion, sample_vec3};
use super::{M2AnimationClock, M2BonePoseError};

/// Per-hand selection for the model-authored `HandsClosed` finger pose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2FingerPoseHands {
    /// Do not replace either finger tree.
    None,
    /// Replace key-bone groups 8 through 12.
    Right,
    /// Replace key-bone groups 13 through 17.
    Left,
    /// Replace both authored finger trees.
    Both,
}

impl M2FingerPoseHands {
    /// Returns whether this mask includes one requested hand.
    #[must_use]
    pub const fn includes(self, hand: Self) -> bool {
        matches!(
            (self, hand),
            (Self::Right | Self::Both, Self::Right) | (Self::Left | Self::Both, Self::Left)
        )
    }
}

/// One complete model-bone matrix palette ready for GPU upload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct M2BonePose {
    transforms: Vec<Mat4>,
    local: Vec<Mat4>,
    states: Vec<u8>,
    sequence_clocks: Vec<Option<M2AnimationClock>>,
    identity_pose: bool,
}

/// Instance-owned modifications applied while composing the authored pose.
#[derive(Clone, Copy, Debug, Default)]
pub struct M2BonePoseOverrides<'a> {
    /// Billboard exceptions selected by the character compositor.
    pub model_oriented_billboard_bones: &'a [bool],
    /// An authored held-item pose restricted to the selected finger trees.
    pub finger_pose: Option<(M2AnimationClock, M2FingerPoseHands)>,
    /// Additional local transforms indexed by the model's semantic key bones.
    /// Missing key bones are ignored, as in the native model setter.
    pub bone_transforms: &'a [(u16, Mat4)],
    /// Sequence clocks rooted at semantic bones, inherited by descendants.
    /// The nearest root wins; later entries replace earlier entries at a root.
    pub bone_sequences: &'a [(u16, M2AnimationClock)],
}

/// Precomputed camera transforms shared by every billboard bone in a pose.
#[derive(Clone, Copy)]
struct BillboardView {
    model_view: Mat4,
    inverse_model_view: Mat4,
}

#[derive(Clone, Copy)]
struct ResolvedFingerPose {
    clock: M2AnimationClock,
    hands: M2FingerPoseHands,
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
        Self::compose_inner(animations, clock, None, &[])
    }

    /// Samples every bone and applies stock's camera-relative billboards.
    ///
    /// `model_view` must transform from model space into the active camera's
    /// view space. Ordinary world callers form it as `frame.view() * model`.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] for invalid animation selection or a
    /// non-finite/non-invertible model-view transform.
    pub fn compose_with_model_view(
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
    ) -> Result<Self, M2BonePoseError> {
        Self::compose_with_model_view_and_orientation_mask(animations, clock, model_view, &[])
    }

    /// Samples every bone while keeping selected billboard bones model-oriented.
    ///
    /// Build 12340's character compositor uses this exception for the bones
    /// weighted by the selected 17xx eye card. Other billboard bones retain
    /// the ordinary camera-relative path. A mask shorter than the bone table
    /// leaves every missing entry camera-relative.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] for invalid animation selection or a
    /// non-finite/non-invertible model-view transform.
    pub fn compose_with_model_view_and_orientation_mask(
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
        model_oriented_billboard_bones: &[bool],
    ) -> Result<Self, M2BonePoseError> {
        let mut pose = Self::default();
        pose.recompose_with_model_view_and_orientation_mask(
            animations,
            clock,
            model_view,
            model_oriented_billboard_bones,
        )?;
        Ok(pose)
    }

    /// Rebuilds a camera-aware palette while retaining its backing storage.
    ///
    /// # Errors
    ///
    /// Returns the same errors as
    /// [`Self::compose_with_model_view_and_orientation_mask`].
    pub fn recompose_with_model_view_and_orientation_mask(
        &mut self,
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
        model_oriented_billboard_bones: &[bool],
    ) -> Result<(), M2BonePoseError> {
        if !finite_matrix(model_view) {
            return Err(M2BonePoseError::InvalidModelView);
        }
        let determinant = model_view.determinant();
        if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
            return Err(M2BonePoseError::InvalidModelView);
        }
        self.recompose_inner(
            animations,
            clock,
            Some(BillboardView {
                model_view,
                inverse_model_view: model_view.inverse(),
            }),
            model_oriented_billboard_bones,
            None,
            &[],
            &[],
        )
    }

    /// Samples the body sequence while replacing only held-item finger tracks.
    ///
    /// The overlay is applied only when its selected sequence authors keys for
    /// a track in key-bone groups 8..=17 (including unnamed descendants).
    /// Global-sequence tracks remain driven by the process clock.
    #[allow(clippy::too_many_arguments)]
    pub fn recompose_with_model_view_orientation_and_finger_pose(
        &mut self,
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
        model_oriented_billboard_bones: &[bool],
        finger_pose: Option<(M2AnimationClock, M2FingerPoseHands)>,
    ) -> Result<(), M2BonePoseError> {
        self.recompose_with_overrides(
            animations,
            clock,
            model_view,
            M2BonePoseOverrides {
                model_oriented_billboard_bones,
                finger_pose,
                bone_transforms: &[],
                bone_sequences: &[],
            },
        )
    }

    /// Composes instance bone transforms with the authored tracks and hierarchy.
    /// Native `82FD25` applies the extra matrix after authored rotation/scale
    /// in row-vector space, before pivot/translation and parent composition.
    pub fn recompose_with_overrides(
        &mut self,
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Mat4,
        overrides: M2BonePoseOverrides<'_>,
    ) -> Result<(), M2BonePoseError> {
        if !finite_matrix(model_view) {
            return Err(M2BonePoseError::InvalidModelView);
        }
        let determinant = model_view.determinant();
        if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
            return Err(M2BonePoseError::InvalidModelView);
        }
        self.recompose_inner(
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
        )
    }

    /// Shares animation selection and hierarchy work between pose paths.
    fn compose_inner(
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Option<BillboardView>,
        model_oriented_billboard_bones: &[bool],
    ) -> Result<Self, M2BonePoseError> {
        let mut pose = Self::default();
        pose.recompose_inner(
            animations,
            clock,
            model_view,
            model_oriented_billboard_bones,
            None,
            &[],
            &[],
        )?;
        Ok(pose)
    }

    /// Shares animation selection and hierarchy work while retaining vectors.
    #[allow(clippy::too_many_arguments)]
    fn recompose_inner(
        &mut self,
        animations: &M2AnimationSet,
        clock: M2AnimationClock,
        model_view: Option<BillboardView>,
        model_oriented_billboard_bones: &[bool],
        finger_pose: Option<(M2AnimationClock, M2FingerPoseHands)>,
        bone_transforms: &[(u16, Mat4)],
        bone_sequences: &[(u16, M2AnimationClock)],
    ) -> Result<(), M2BonePoseError> {
        let clock = clock.resolve(animations)?;
        self.sequence_clocks.resize(animations.bones().len(), None);
        self.sequence_clocks.fill(None);
        for &(key, clock) in bone_sequences {
            let clock = clock.resolve(animations)?;
            if let Some(index) = animations
                .key_bone_lookup()
                .get(usize::from(key))
                .copied()
                .flatten()
            {
                self.sequence_clocks[usize::from(index)] = Some(clock);
            }
        }
        if !bone_sequences.is_empty() {
            // Nearest root wins; later overrides at a root replaced earlier ones above.
            for &index in animations.bone_parent_order() {
                if self.sequence_clocks[index].is_none() {
                    self.sequence_clocks[index] = animations.bones()[index]
                        .parent()
                        .and_then(|parent| self.sequence_clocks[usize::from(parent)]);
                }
            }
        }
        for (key_bone, transform) in bone_transforms {
            if !finite_matrix(*transform) {
                return Err(M2BonePoseError::InvalidBoneTransform {
                    key_bone: *key_bone,
                });
            }
        }
        let finger_pose = finger_pose
            .map(|(finger_clock, hands)| {
                Ok::<ResolvedFingerPose, M2BonePoseError>(ResolvedFingerPose {
                    clock: finger_clock.resolve(animations)?,
                    hands,
                })
            })
            .transpose()?;
        if model_view.is_none()
            && let Some((bone, _)) = animations
                .bones()
                .iter()
                .enumerate()
                .find(|(_index, bone)| bone.flags() & 0x78 != 0)
        {
            return Err(M2BonePoseError::BillboardViewRequired { bone });
        }

        // Empty tracks have the same identity palette for every sequence and
        // owner. Clocks and overrides above still validate, while all effect,
        // event and attachment consumers continue through their ordinary paths.
        if animations.has_identity_bone_pose() && bone_transforms.is_empty() {
            let count = animations.bones().len();
            if !self.identity_pose || self.transforms.len() != count {
                self.transforms.resize(count, Mat4::IDENTITY);
                self.transforms.fill(Mat4::IDENTITY);
                self.local.resize(count, Mat4::IDENTITY);
                self.local.fill(Mat4::IDENTITY);
                self.states.resize(count, 2);
                self.states.fill(2);
            }
            self.identity_pose = true;
            return Ok(());
        }
        self.identity_pose = false;
        self.local.resize(animations.bones().len(), Mat4::IDENTITY);
        for (index, bone) in animations.bones().iter().enumerate() {
            let clock = self.sequence_clocks[index].unwrap_or(clock);
            let finger_pose =
                finger_pose.filter(|pose| pose.hands.includes(finger_pose_hand(animations, index)));
            let translation = sample_vec3(
                animations,
                bone.translation(),
                track_clock(bone.translation(), clock, finger_pose),
                Vec3::ZERO,
            );
            let rotation = sample_quaternion(
                animations,
                bone.rotation(),
                track_clock(bone.rotation(), clock, finger_pose),
            );
            let scale = sample_vec3(
                animations,
                bone.scale(),
                track_clock(bone.scale(), clock, finger_pose),
                Vec3::ONE,
            );
            let mut rotation_scale = quaternion_matrix(rotation) * Mat4::from_scale(scale);
            if let Some((_, transform)) = bone_transforms.iter().rev().find(|(key, _)| {
                animations
                    .key_bone_lookup()
                    .get(usize::from(*key))
                    .copied()
                    .flatten()
                    .is_some_and(|bone| usize::from(bone) == index)
            }) {
                rotation_scale = *transform * rotation_scale;
            }
            self.local[index] = Mat4::from_translation(bone.pivot())
                * Mat4::from_translation(translation)
                * rotation_scale
                * Mat4::from_translation(-bone.pivot());
        }

        self.transforms.resize(self.local.len(), Mat4::IDENTITY);
        self.transforms.fill(Mat4::IDENTITY);
        self.states.resize(self.local.len(), 0);
        self.states.fill(0);
        for index in 0..self.local.len() {
            compose_bone(
                index,
                animations,
                &self.local,
                &mut self.transforms,
                &mut self.states,
                model_view,
                model_oriented_billboard_bones,
            );
        }
        Ok(())
    }

    /// Returns model-space transforms indexed exactly like the M2 bone array.
    #[must_use]
    pub fn transforms(&self) -> &[Mat4] {
        &self.transforms
    }

    /// Resolves the stock particle generator basis at its animated world origin.
    ///
    /// Build 12340 `0x008309C0` appends the authored emitter position to its
    /// bone, applies the model placement, then appends the fixed basis stored
    /// at `0x00D411E0`: generator +X maps to bone +Y, +Y to -X, and +Z to +Z.
    /// This changes launch directions and local cards without orbiting the
    /// authored emitter origin. Both world-space births and model-space
    /// presentation must use this same transform.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError::ParticleBoneIndex`] when the emitter's bone
    /// is absent from this palette. Unbound emitters use the model placement
    /// with the same generator remap.
    pub fn particle_emitter_transform(
        &self,
        emitter: &M2ParticleEmitter,
        model_transform: Mat4,
    ) -> Result<Mat4, M2BonePoseError> {
        let bone = match emitter.bone_index() {
            Some(index) => self.transforms.get(usize::from(index)).copied().ok_or(
                M2BonePoseError::ParticleBoneIndex {
                    requested: index,
                    available: self.transforms.len(),
                },
            )?,
            None => Mat4::IDENTITY,
        };
        let mut transform = model_transform * bone * Mat4::from_translation(emitter.position());
        // Exact column exchange preserves the executable's zero/one matrix
        // instead of introducing trigonometric rounding at a right angle.
        let original_x = transform.x_axis;
        transform.x_axis = transform.y_axis;
        transform.y_axis = -original_x;
        Ok(transform)
    }

    /// Resolves one enabled child-model attachment in world space.
    ///
    /// The attachment's enable channel uses the same resolved animation clock
    /// as its owning body. Its stable local position is appended after the
    /// animated parent bone, then the complete parent placement is applied.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] when the animation selection is invalid or
    /// the supplied attachment does not belong to this bone palette.
    pub fn attachment_transform(
        &self,
        animations: &M2AnimationSet,
        attachment: &M2Attachment,
        clock: M2AnimationClock,
        model_transform: Mat4,
    ) -> Result<Option<Mat4>, M2BonePoseError> {
        let clock = clock.resolve(animations)?;
        let enabled = sample_discrete(animations, attachment.enabled(), clock, 1_u8);
        if enabled == 0 {
            return Ok(None);
        }
        let bone_index = usize::from(attachment.bone_index());
        let bone = self.transforms.get(bone_index).copied().ok_or(
            M2BonePoseError::AttachmentBoneIndex {
                requested: attachment.bone_index(),
                available: self.transforms.len(),
            },
        )?;
        Ok(Some(
            model_transform * bone * Mat4::from_translation(attachment.position()),
        ))
    }
}

/// A finger overlay replaces only tracks that have authored finger keys.
fn track_clock<T>(
    track: &M2Track<T>,
    clock: M2AnimationClock,
    finger_pose: Option<ResolvedFingerPose>,
) -> M2AnimationClock {
    finger_pose
        .filter(|pose| {
            track.global_sequence().is_none()
                && track
                    .channels()
                    .get(pose.clock.sequence())
                    .is_some_and(|channel| !channel.timestamps_ms().is_empty())
        })
        .map_or(clock, |pose| pose.clock.inherit_secondary(clock))
}

/// Finger ancestry was resolved once with the immutable bone hierarchy.
fn finger_pose_hand(animations: &M2AnimationSet, index: usize) -> M2FingerPoseHands {
    match animations.finger_key_bone(index) {
        Some(8..=12) => M2FingerPoseHands::Right,
        Some(13..=17) => M2FingerPoseHands::Left,
        _ => M2FingerPoseHands::None,
    }
}

/// Resolves an arbitrarily ordered but cycle-free hierarchy once per bone.
fn compose_bone(
    index: usize,
    animations: &M2AnimationSet,
    local: &[Mat4],
    transforms: &mut [Mat4],
    states: &mut [u8],
    model_view: Option<BillboardView>,
    model_oriented_billboard_bones: &[bool],
) {
    if states[index] == 2 {
        return;
    }
    states[index] = 1;
    if let Some(parent) = animations.bones()[index].parent().map(usize::from) {
        if states[parent] == 0 {
            compose_bone(
                parent,
                animations,
                local,
                transforms,
                states,
                model_view,
                model_oriented_billboard_bones,
            );
        }
        transforms[index] =
            inherited_parent_transform(transforms[parent], animations.bones()[index].flags())
                * local[index];
    } else {
        transforms[index] = local[index];
    }
    if let Some(view) = model_view {
        let flags = animations.bones()[index].flags() & 0x78;
        if flags != 0
            && !model_oriented_billboard_bones
                .get(index)
                .copied()
                .unwrap_or(false)
        {
            transforms[index] = billboard_transform(
                transforms[index],
                local[index],
                animations.bones()[index].pivot(),
                animations.bones()[index].flags(),
                view,
            );
        }
    }
    states[index] = 2;
}

/// Applies build-12340's spherical or axis-constrained view-space basis.
fn billboard_transform(
    model_bone: Mat4,
    local_bone: Mat4,
    pivot: Vec3,
    flags: u32,
    view: BillboardView,
) -> Mat4 {
    let mut view_bone = view.model_view * model_bone;
    let original = view_bone;
    let scales = Vec3::new(
        view_bone.x_axis.truncate().length(),
        view_bone.y_axis.truncate().length(),
        view_bone.z_axis.truncate().length(),
    );
    let set_axis = |matrix: &mut Mat4, column: usize, axis: Vec3| {
        let scale = scales[column];
        *matrix.col_mut(column) = (axis * scale).extend(0.0);
    };

    match flags & 0x78 {
        0x8 => {
            // Animated cards preserve authored local orientation inside the
            // stock view-space remap while cancelling parent/view rotation.
            if flags & 0x280 != 0 {
                for column in 0..3 {
                    let authored = local_bone.col(column).truncate();
                    let fallback = match column {
                        0 => Vec3::new(0.0, 0.0, -1.0),
                        1 => Vec3::X,
                        _ => Vec3::Y,
                    };
                    set_axis(
                        &mut view_bone,
                        column,
                        normalize_or(Vec3::new(authored.y, authored.z, -authored.x), fallback),
                    );
                }
            } else {
                set_axis(&mut view_bone, 0, Vec3::new(0.0, 0.0, -1.0));
                set_axis(&mut view_bone, 1, Vec3::X);
                set_axis(&mut view_bone, 2, Vec3::Y);
            }
        }
        0x10 => {
            let x = normalize_or(view_bone.x_axis.truncate(), Vec3::X);
            let y = normalize_or(Vec3::new(x.y, -x.x, 0.0), Vec3::Y);
            let z = normalize_or(y.cross(x), Vec3::Z);
            set_axis(&mut view_bone, 0, x);
            set_axis(&mut view_bone, 1, y);
            set_axis(&mut view_bone, 2, z);
        }
        0x20 => {
            let y = normalize_or(view_bone.y_axis.truncate(), Vec3::Y);
            let x = normalize_or(Vec3::new(-y.y, y.x, 0.0), Vec3::X);
            let z = normalize_or(y.cross(x), Vec3::Z);
            set_axis(&mut view_bone, 0, x);
            set_axis(&mut view_bone, 1, y);
            set_axis(&mut view_bone, 2, z);
        }
        0x40 => {
            let z = normalize_or(view_bone.z_axis.truncate(), Vec3::Z);
            let y = normalize_or(Vec3::new(z.y, -z.x, 0.0), Vec3::Y);
            let x = normalize_or(z.cross(y), Vec3::X);
            set_axis(&mut view_bone, 0, x);
            set_axis(&mut view_bone, 1, y);
            set_axis(&mut view_bone, 2, z);
        }
        _ => {}
    }

    // Replacing orientation must not orbit the card around the model origin.
    let transformed_pivot = original * pivot.extend(1.0);
    let rotated_pivot = view_bone * pivot.extend(0.0);
    view_bone.w_axis = transformed_pivot - rotated_pivot;
    view_bone.w_axis.w = 1.0;
    view.inverse_model_view * view_bone
}

/// Preserves stock's stable billboard axes when authored scale is zero.
fn normalize_or(value: Vec3, fallback: Vec3) -> Vec3 {
    let length_squared = value.length_squared();
    if length_squared.is_finite() && length_squared > 1.0e-6 {
        value / length_squared.sqrt()
    } else {
        fallback
    }
}

/// Ensures matrix inversion cannot inject non-finite billboard transforms.
fn finite_matrix(value: Mat4) -> bool {
    value.to_cols_array().into_iter().all(f32::is_finite)
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
