//! Placement-owned ribbon simulation consumes a complete admitted pose.

use super::super::super::RuntimeTerrainFrameError;
use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2CameraEffectScale, M2RibbonControlPoint, M2RibbonPose,
    M2RibbonTrail,
};

/// Advances every shared declaration through its placement-owned edge history.
#[allow(clippy::too_many_arguments)] // One authored sample supplies all ribbon consumers.
pub(in super::super::super) fn advance_ribbons(
    model: &DecodedM2Model,
    transform: Mat4,
    ribbons: &mut [M2RibbonTrail],
    bone_pose: &M2BonePose,
    clock: M2AnimationClock,
    delta_seconds: f32,
    effect_scale: M2CameraEffectScale,
    instance_alpha: f32,
) -> Result<(), RuntimeTerrainFrameError> {
    let emitters = model.animations().ribbons();
    if ribbons.len() != emitters.len() {
        return Err(RuntimeTerrainFrameError::M2RibbonTrailCount {
            model: model.path().clone(),
            trail_count: ribbons.len(),
            emitter_count: emitters.len(),
        });
    }
    for (ribbon_index, (emitter, trail)) in emitters.iter().zip(ribbons.iter_mut()).enumerate() {
        let bone = match emitter.bone_index() {
            Some(bone_index) => bone_pose
                .transforms()
                .get(bone_index as usize)
                .copied()
                .ok_or_else(|| RuntimeTerrainFrameError::M2RibbonBoneIndex {
                    model: model.path().clone(),
                    ribbon_index,
                    bone_index,
                })?,
            None => Mat4::IDENTITY,
        };
        // Stock appends the emitter-local translation to the animated bone,
        // then composes the placement. Its column-major matrix passes Y as the
        // strip width axis and Z as the interpolation tangent.
        let transform = transform * bone * Mat4::from_translation(emitter.position());
        let control = M2RibbonControlPoint::new(
            transform.w_axis.truncate(),
            transform.y_axis.truncate() * effect_scale.factor(),
            transform.z_axis.truncate(),
        );
        let pose = M2RibbonPose::sample(model.animations(), emitter, clock)?
            .with_instance_alpha(instance_alpha);
        trail.advance(delta_seconds, control, pose)?;
    }
    Ok(())
}
