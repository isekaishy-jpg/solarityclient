//! Per-draw M2 material animation sampling.

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::DecodedM2Model;

use crate::model::M2MeshPlan;

use super::sample::{sample_quaternion, sample_scalar, sample_vec3};
use super::{M2AnimationClock, M2MaterialPoseError};

/// Animated material values shared by GPU draw preparation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2MaterialPose {
    texture_transforms: [Mat4; 2],
    mesh_color: Vec4,
}

impl M2MaterialPose {
    /// Samples one validated SKIN draw at the supplied local/global clock.
    ///
    /// # Errors
    ///
    /// Returns [`M2MaterialPoseError`] for cross-model plans, absent draw or
    /// lookup entries, and unavailable animation sequences.
    pub fn sample(
        model: &DecodedM2Model,
        plan: &M2MeshPlan,
        draw_index: usize,
        clock: M2AnimationClock,
    ) -> Result<Self, M2MaterialPoseError> {
        if plan.path() != model.path() {
            return Err(M2MaterialPoseError::ModelMismatch);
        }
        let draw = plan
            .draws()
            .get(draw_index)
            .ok_or(M2MaterialPoseError::DrawIndex {
                requested: draw_index,
                available: plan.draws().len(),
            })?;
        let sequence = clock.resolve(model.animations())?;
        let animation_time_ms = clock.animation_time_ms();
        let global_time_ms = clock.global_time_ms();
        let animations = model.animations();

        let mut mesh_color = Vec4::ONE;
        let color_index = draw.batch().color_index;
        if color_index != u16::MAX {
            let color = animations.colors().get(usize::from(color_index)).ok_or(
                M2MaterialPoseError::ColorIndex {
                    requested: color_index,
                    available: animations.colors().len(),
                },
            )?;
            mesh_color = sample_vec3(
                animations,
                color.color(),
                sequence,
                animation_time_ms,
                global_time_ms,
                Vec3::ONE,
            )
            .clamp(Vec3::ZERO, Vec3::splat(16.0))
            .extend(
                sample_scalar(
                    animations,
                    color.alpha(),
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                    1.0,
                )
                .clamp(0.0, 1.0),
            );
        }

        let batch = draw.batch();
        if usize::from(batch.texture_count) > 2 {
            return Err(M2MaterialPoseError::TextureStageCount {
                requested: batch.texture_count,
            });
        }
        if batch.texture_weight_combo_index != u16::MAX {
            let lookup_index = usize::from(batch.texture_weight_combo_index);
            let weight_index = *model.texture_weight_lookup().get(lookup_index).ok_or(
                M2MaterialPoseError::TextureWeightLookup {
                    requested: lookup_index,
                    available: model.texture_weight_lookup().len(),
                },
            )?;
            if weight_index != u16::MAX {
                let weight = animations
                    .texture_weights()
                    .get(usize::from(weight_index))
                    .ok_or(M2MaterialPoseError::TextureWeightIndex {
                        requested: weight_index,
                        available: animations.texture_weights().len(),
                    })?;
                mesh_color.w *= sample_scalar(
                    animations,
                    weight.weight(),
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                    1.0,
                )
                .clamp(0.0, 1.0);
            }
        }

        let mut texture_transforms = [Mat4::IDENTITY; 2];
        if batch.texture_transform_combo_index != u16::MAX {
            for (stage, destination) in texture_transforms
                .iter_mut()
                .enumerate()
                .take(usize::from(batch.texture_count))
            {
                let lookup_index = usize::from(batch.texture_transform_combo_index) + stage;
                let transform_index = *model.texture_transform_lookup().get(lookup_index).ok_or(
                    M2MaterialPoseError::TextureTransformLookup {
                        requested: lookup_index,
                        available: model.texture_transform_lookup().len(),
                    },
                )?;
                if transform_index == u16::MAX {
                    continue;
                }
                let transform = animations
                    .texture_transforms()
                    .get(usize::from(transform_index))
                    .ok_or(M2MaterialPoseError::TextureTransformIndex {
                        requested: transform_index,
                        available: animations.texture_transforms().len(),
                    })?;
                let translation = sample_vec3(
                    animations,
                    transform.translation(),
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                    Vec3::ZERO,
                );
                let rotation = sample_quaternion(
                    animations,
                    transform.rotation(),
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                );
                let scale = sample_vec3(
                    animations,
                    transform.scale(),
                    sequence,
                    animation_time_ms,
                    global_time_ms,
                    Vec3::ONE,
                );
                *destination = texture_transform(translation, rotation, scale);
            }
        }
        Ok(Self {
            texture_transforms,
            mesh_color,
        })
    }

    /// Returns both animated texture-stage matrices.
    #[must_use]
    pub const fn texture_transforms(self) -> [Mat4; 2] {
        self.texture_transforms
    }

    /// Returns animated RGB and the color/weight alpha product.
    #[must_use]
    pub const fn mesh_color(self) -> Vec4 {
        self.mesh_color
    }
}

/// Applies stock's half-texel pivot to rotation and scale before translation.
fn texture_transform(translation: Vec3, rotation: glam::Quat, scale: Vec3) -> Mat4 {
    let pivot = Vec3::new(0.5, 0.5, 0.0);
    Mat4::from_translation(pivot)
        * Mat4::from_quat(rotation)
        * Mat4::from_translation(-pivot)
        * Mat4::from_translation(pivot)
        * Mat4::from_scale(scale)
        * Mat4::from_translation(-pivot)
        * Mat4::from_translation(translation)
}
