//! Animated light sampling for one M2 placement.

use glam::{Mat4, Vec3};
use solarity_asset::{M2AnimationSet, M2Light, M2LightKind};

use crate::{M2DirectionalLight, M2PointLight};

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
    Ok(sample_m2_lights(animations, pose, clock, model_transform)?.directional)
}

/// Every visible directional and point light sampled for one M2 placement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct M2SampledLights {
    /// Directional sources in authored order.
    pub directional: Vec<M2DirectionalLight>,
    /// Point sources ordered nearest to the model placement first.
    pub points: Vec<M2PointLight>,
}

/// Samples all visible authored M2 lights in scene coordinates.
///
/// Point sources are stably ordered by distance to the model placement, as in
/// build 12340, before the caller applies the fixed four-light hardware bound.
///
/// # Errors
///
/// Returns [`M2BonePoseError`] when the animation clock cannot resolve or an
/// authored light references a bone absent from the supplied pose.
pub fn sample_m2_lights(
    animations: &M2AnimationSet,
    pose: &M2BonePose,
    clock: M2AnimationClock,
    model_transform: Mat4,
) -> Result<M2SampledLights, M2BonePoseError> {
    let mut sampled = M2SampledLights::default();
    sample_m2_lights_into(
        animations,
        pose,
        clock,
        model_transform,
        &mut sampled.directional,
        &mut sampled.points,
    )?;
    Ok(sampled)
}

/// Samples visible M2 lights directly into retained caller-owned storage.
///
/// Both destinations are cleared before use. Their allocations are retained,
/// and point lights receive the same stable distance ordering as
/// [`sample_m2_lights`].
///
/// # Errors
///
/// Returns the same failures as [`sample_m2_lights`].
pub fn sample_m2_lights_into(
    animations: &M2AnimationSet,
    pose: &M2BonePose,
    clock: M2AnimationClock,
    model_transform: Mat4,
    directional: &mut Vec<M2DirectionalLight>,
    points: &mut Vec<M2PointLight>,
) -> Result<(), M2BonePoseError> {
    let sequence = clock.resolve(animations)?;
    directional.clear();
    points.clear();
    directional.reserve(animations.lights().len());
    points.reserve(animations.lights().len());
    for light in animations.lights() {
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
        let (ambient, diffuse) = sample_light_colors(animations, light, sequence, clock);
        let mut position = light
            .position()
            .extend(if light.kind() == M2LightKind::Point {
                1.0
            } else {
                0.0
            });
        if let Some(bone_index) = light.bone_index() {
            position = pose
                .transforms()
                .get(usize::from(bone_index))
                .copied()
                .ok_or(M2BonePoseError::LightBoneIndex {
                    requested: bone_index,
                    available: pose.transforms().len(),
                })?
                * position;
        }
        position = model_transform * position;
        if light.kind() == M2LightKind::Point {
            points.push(M2PointLight::new(position.truncate(), ambient, diffuse));
        } else {
            let mut direction = position.truncate();
            if direction.length() > f32::EPSILON {
                direction = direction.normalize();
            }
            directional.push(M2DirectionalLight::new(direction, ambient, diffuse));
        }
    }
    let origin = model_transform.transform_point3(Vec3::ZERO);
    points.sort_by(|left, right| {
        left.position()
            .distance_squared(origin)
            .total_cmp(&right.position().distance_squared(origin))
    });
    Ok(())
}

fn sample_light_colors(
    animations: &M2AnimationSet,
    light: &M2Light,
    sequence: usize,
    clock: M2AnimationClock,
) -> (Vec3, Vec3) {
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
    (ambient, diffuse)
}
