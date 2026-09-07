//! Scheduled remote path advancement before model and sound synchronization.

use shipyard::{Get, IntoIter, ViewMut};
use solarity_ecs::{ActiveWorld, WorldMovementState, WorldStateError, WorldTransform};

use super::{MovementSpline, MovementSplineError};

/// Replaces or removes the path owned by an existing object lifetime.
///
/// # Errors
/// Rejects an unknown GUID before changing component ownership.
pub fn set_world_movement_spline(
    world: &mut ActiveWorld,
    guid: u64,
    spline: Option<MovementSpline>,
) -> Result<(), WorldStateError> {
    let entity = world
        .entity_by_guid(guid)
        .ok_or(WorldStateError::UnknownObject { guid })?;
    match spline {
        Some(spline) => world.storage_mut().add_component(entity, (spline,)),
        None => {
            world.storage_mut().remove::<(MovementSpline,)>(entity);
        }
    }
    Ok(())
}

/// Advances retained remote paths at the caller's explicit client clock.
/// Each result reaches ECS before animation and positional audio read the unit.
///
/// # Errors
/// Returns an invalid native path/trajectory error without inventing movement.
pub fn advance_world_movement_splines(
    world: &ActiveWorld,
    now_ms: u32,
) -> Result<(), MovementSplineError> {
    advance_paths(world, now_ms, None)
}

/// Advances a single remote owner before a replacement packet uses its position.
///
/// # Errors
/// Returns an invalid native trajectory error without replacing the path.
pub fn advance_world_movement_spline(
    world: &ActiveWorld,
    guid: u64,
    now_ms: u32,
) -> Result<(), MovementSplineError> {
    let Some(entity) = world.entity_by_guid(guid) else {
        return Ok(());
    };
    advance_paths(world, now_ms, Some(entity))
}

/// Borrows each component storage once for a frame or selected unit update.
fn advance_paths(
    world: &ActiveWorld,
    now_ms: u32,
    selected: Option<shipyard::EntityId>,
) -> Result<(), MovementSplineError> {
    world.storage().run(
        |mut paths: ViewMut<MovementSpline>,
         mut movements: ViewMut<WorldMovementState>,
         mut transforms: ViewMut<WorldTransform>| {
            for (entity, (path, movement)) in (&mut paths, &mut movements).iter().with_id() {
                if selected.is_some_and(|selected| entity != selected) {
                    continue;
                }
                if entity == world.local_player() {
                    continue;
                }
                // The runtime passenger timeline owns attached paths and their
                // resident parent frames. Its local point requires world projection.
                if movement.transport_guid().is_some() {
                    continue;
                }
                let Ok(current) = (&transforms).get(entity).copied() else {
                    continue;
                };
                let (transform, next_movement) =
                    path.advance_movement(now_ms, *movement, current, |guid| {
                        let target = world.entity_by_guid(guid)?;
                        (&transforms)
                            .get(target)
                            .ok()
                            .map(|transform| transform.position())
                    })?;
                *movement = next_movement;
                if let Ok(mut stored) = (&mut transforms).get(entity) {
                    *stored = transform;
                }
            }
            Ok(())
        },
    )
}
