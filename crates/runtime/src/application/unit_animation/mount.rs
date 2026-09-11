//! The independent mount uses the unit's movement and its own sequence metadata.

use super::*;

pub(in crate::application) fn select_mount_animation(
    owner: Option<&UnitAnimationBehavior>,
    playback: &mut M2Playback,
    model: &DecodedM2Model,
    animation_id: u16,
    scene_time_ms: f32,
    random: &mut CrtRand,
) -> Result<(), RuntimeTerrainFrameError> {
    let timing = owner.map_or((1., 0), |owner| {
        model_sequence_timing(
            model,
            playback,
            animation_id,
            owner.input.get(),
            scene_time_ms as u32,
        )
    });
    playback.select_mount_animation(model, animation_id, timing, scene_time_ms, random)
}
