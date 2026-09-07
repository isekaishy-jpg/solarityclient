//! Network snapshot to retained systems-owned spline projection.

use glam::Vec3;
use solarity_ecs::ActiveWorld;
use solarity_network::{MovementSplineFacing as WireFacing, ObjectMovementUpdate};
use solarity_systems::{
    MovementSpline, MovementSplineDefinition, MovementSplineFacing, set_world_movement_spline,
};

use super::{GameplayUpdateError, movement_state, movement_transform};

/// Installs complete movement state, replacing the previous path on every
/// admitted update. Object-create and local-echo guards stay with the caller.
pub(super) fn install(
    world: &mut ActiveWorld,
    guid: u64,
    update: &ObjectMovementUpdate,
    receipt_ms: u32,
) -> Result<(), GameplayUpdateError> {
    let Some(mut movement) = movement_state(update) else {
        return Ok(());
    };
    let spline = if let Some(source) = update.spline() {
        let definition = MovementSplineDefinition {
            flags: source.flags,
            facing: match source.facing {
                WireFacing::Direction => MovementSplineFacing::Direction,
                WireFacing::Angle(angle) => MovementSplineFacing::Angle(angle),
                WireFacing::Target(guid) => MovementSplineFacing::Target(guid),
                WireFacing::Point(point) => MovementSplineFacing::Point(Vec3::from_array(point)),
            },
            id: source.id,
            elapsed_ms: source.elapsed_ms,
            duration_ms: source.duration_ms,
            duration_scale: source.timing_parameters[0],
            next_duration_scale: source.timing_parameters[1],
            vertical_acceleration: source.timing_parameters[2],
            effect_start_ms: source.effect_start_ms,
            nodes: source.nodes.iter().copied().map(Vec3::from_array).collect(),
            destination: Vec3::from_array(source.destination),
        };
        let transform = movement_transform(update)
            .ok_or(solarity_systems::MovementSplineError::InvalidDefinition)?;
        let spline = MovementSpline::new(definition, receipt_ms, transform)?;
        movement = movement.with_spline(spline.motion());
        Some(spline)
    } else {
        None
    };
    set_world_movement_spline(world, guid, spline)?;
    world.update_movement(guid, movement)?;
    crate::application::player_movement::remote::baseline(world, guid, receipt_ms);
    Ok(())
}
