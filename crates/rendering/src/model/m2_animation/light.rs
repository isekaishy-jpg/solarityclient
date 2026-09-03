//! Animated light sampling for one M2 placement.

use glam::{Mat4, Vec3};
use solarity_asset::{M2AnimationSet, M2LightKind};

use crate::M2DirectionalLight;

use super::sample::{sample_discrete, sample_scalar, sample_vec3};
use super::{M2AnimationClock, M2BonePose, M2BonePoseError};

/// Samples every visible authored directional light in scene coordinates.
///
/// Build 12340 transforms the light basis through its optional owning bone and
/// the M2 placement before adding it to the per-model lighting accumulator.
/// Returned vectors retain D3D's ray direction; the sunlight merger performs
/// the one required inversion into the shader's surface-to-light convention.
///
/// # Errors
///
/// Returns [`M2BonePoseError`] when the animation clock cannot resolve or an
/// authored light references a bone absent from the supplied pose.
pub fn sample_m2_directional_lights(
    animations: &M2AnimationSet,
    pose: &M2BonePose,
    clock: M2AnimationClock,
    model_transform: Mat4,
) -> Result<Vec<M2DirectionalLight>, M2BonePoseError> {
    let sequence = clock.resolve(animations)?;
    let mut sampled = Vec::new();
    for light in animations
        .lights()
        .iter()
        .filter(|light| light.kind() == M2LightKind::Directional)
    {
        let visible = sample_discrete(
            animations,
            light.visibility(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            1_u8,
        );
        if visible == 0 {
            continue;
        }
        let ambient_intensity = sample_scalar(
            animations,
            light.ambient_intensity(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            1.0,
        )
        .max(0.0);
        let diffuse_intensity = sample_scalar(
            animations,
            light.diffuse_intensity(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            1.0,
        )
        .max(0.0);
        let ambient = (sample_vec3(
            animations,
            light.ambient_color(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            Vec3::ONE,
        ) * ambient_intensity)
            .max(Vec3::ZERO);
        let diffuse = (sample_vec3(
            animations,
            light.diffuse_color(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            Vec3::ONE,
        ) * diffuse_intensity)
            .max(Vec3::ZERO);
        let mut direction = light.position().extend(0.0);
        if let Some(bone_index) = light.bone_index() {
            direction = pose
                .transforms()
                .get(usize::from(bone_index))
                .copied()
                .ok_or(M2BonePoseError::LightBoneIndex {
                    requested: bone_index,
                    available: pose.transforms().len(),
                })?
                * direction;
        }
        direction = model_transform * direction;
        let mut direction = direction.truncate();
        if direction.length() > f32::EPSILON {
            direction = direction.normalize();
        }
        sampled.push(M2DirectionalLight::new(direction, ambient, diffuse));
    }
    Ok(sampled)
}
