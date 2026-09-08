//! Animated light sampling for one M2 placement.

use glam::{Mat4, Vec3};
use solarity_asset::{M2AnimationSet, M2Light, M2LightKind};

use crate::{M2DirectionalLight, M2PointLight};

use super::sample::{sample_discrete, sample_scalar, sample_vec3};
use super::{M2AnimationClock, M2BonePose, M2BonePoseError};

/// Samples every visible authored directional light in scene coordinates.
///
/// Build 12340 transforms the light basis through its owning bone and
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
    directional.clear();
    points.clear();
    directional.reserve(animations.lights().len());
    points.reserve(animations.lights().len());
    sample_lights_with(
        animations,
        pose,
        clock,
        model_transform,
        |_, light| directional.push(light),
        |light| points.push(light),
    )?;
    let origin = model_transform.transform_point3(Vec3::ZERO);
    points.sort_by(|left, right| {
        left.position()
            .distance_squared(origin)
            .total_cmp(&right.position().distance_squared(origin))
    });
    Ok(())
}

/// Samples current lights in authored order for native scene-grid publication.
/// Both destinations are cleared and retain their allocations. Spatial queries
/// choose light order independently for each receiving model. Directional pairs
/// retain their authored index so visibility changes preserve scene list order.
///
/// # Errors
/// Returns the same clock and bone failures as [`sample_m2_lights_into`].
pub fn sample_m2_scene_lights_into(
    animations: &M2AnimationSet,
    pose: &M2BonePose,
    clock: M2AnimationClock,
    model_transform: Mat4,
    directional: &mut Vec<(usize, M2DirectionalLight)>,
    points: &mut Vec<M2PointLight>,
) -> Result<(), M2BonePoseError> {
    directional.clear();
    points.clear();
    directional.reserve(animations.lights().len());
    points.reserve(animations.lights().len());
    sample_lights_with(
        animations,
        pose,
        clock,
        model_transform,
        |index, light| directional.push((index, light)),
        |light| points.push(light),
    )
}

fn sample_lights_with(
    animations: &M2AnimationSet,
    pose: &M2BonePose,
    clock: M2AnimationClock,
    model_transform: Mat4,
    mut directional: impl FnMut(usize, M2DirectionalLight),
    mut points: impl FnMut(M2PointLight),
) -> Result<(), M2BonePoseError> {
    let clock = clock.resolve(animations)?;
    for (index, light) in animations.lights().iter().enumerate() {
        let visible = sample_discrete(animations, light.visibility(), clock, 1_u8);
        if visible == 0 {
            continue;
        }
        let (ambient, diffuse) = sample_light_colors(animations, light, clock);
        // FUN_00828A00 indexes the bone matrix for both light types without a
        // sentinel branch. Reject an unbound light instead of inventing an
        // identity-bone substitution for an invalid stock matrix reference.
        let bone = light
            .bone_index()
            .and_then(|index| pose.transforms().get(usize::from(index)))
            .ok_or(M2BonePoseError::LightBoneIndex {
                requested: light.bone_index().unwrap_or(u16::MAX),
                available: pose.transforms().len(),
            })?;
        if light.kind() == M2LightKind::Point {
            let position =
                model_transform.transform_point3(bone.transform_point3(light.position()));
            points(M2PointLight::new(position, ambient, diffuse));
        } else {
            // 0x00828B07 takes the negative bone Z column, ignoring the light
            // position. Placement transforms it as a vector, not a point.
            let mut direction = model_transform.transform_vector3(-bone.z_axis.truncate());
            // FUN_00834AE0 compares squared length with DAT_009EA27C
            // (0x34800000); tiny vectors retain their authored magnitude.
            if direction.length_squared() > f32::from_bits(0x3480_0000) {
                direction = direction.normalize();
            }
            directional(index, M2DirectionalLight::new(direction, ambient, diffuse));
        }
    }
    Ok(())
}

fn sample_light_colors(
    animations: &M2AnimationSet,
    light: &M2Light,
    clock: M2AnimationClock,
) -> (Vec3, Vec3) {
    let ambient_intensity =
        sample_scalar(animations, light.ambient_intensity(), clock, 1.0).max(0.0);
    let diffuse_intensity =
        sample_scalar(animations, light.diffuse_intensity(), clock, 1.0).max(0.0);
    let ambient = (sample_vec3(animations, light.ambient_color(), clock, Vec3::ONE)
        * ambient_intensity)
        .max(Vec3::ZERO);
    let diffuse = (sample_vec3(animations, light.diffuse_color(), clock, Vec3::ONE)
        * diffuse_intensity)
        .max(Vec3::ZERO);
    (ambient, diffuse)
}
