//! Applies native server path commands to their existing unit lifetime.

use glam::Vec3;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldMovementState, WorldTransform};
use solarity_network::{MonsterMove, MovementSplineFacing as WireFacing};
use solarity_systems::{
    MovementPathRequest, MovementSpline, MovementSplineDefinition, MovementSplineError,
    MovementSplineFacing, PreparedMovementPath, set_world_movement_spline,
};

use super::GameplayUpdateError;

/// Unknown GUIDs are consumed without creating units (`0073F590`). A nonzero
/// transport remains with the pending transport controller instead of placing
/// parent-relative path points directly into world space.
pub(crate) fn apply_monster_move(
    world: &mut ActiveWorld,
    message: &MonsterMove,
    receipt_ms: u32,
    stop_distance_tolerance: f32,
) -> Result<bool, GameplayUpdateError> {
    if !matches!(
        world.object_kind(message.guid),
        Some(ObjectKind::Unit | ObjectKind::Player)
    ) {
        return Ok(false);
    }
    if message.transport.is_some_and(|parent| parent.guid != 0) {
        return Ok(false);
    }
    let current = world
        .object_transform(message.guid)
        .ok_or(GameplayUpdateError::MissingMovement { guid: message.guid })?;
    let movement = world
        .movement_state(message.guid)
        .ok_or(GameplayUpdateError::MissingMovement { guid: message.guid })?;
    let prepared = prepare_monster_move(
        current,
        movement,
        message,
        receipt_ms,
        stop_distance_tolerance,
        |guid| world.object_transform(guid).map(|target| target.position()),
    )?;
    set_world_movement_spline(world, message.guid, prepared.spline)?;
    world.update_transform(message.guid, prepared.transform)?;
    world.update_movement(message.guid, prepared.movement)?;
    crate::application::player_movement::remote::baseline(world, message.guid, receipt_ms);
    Ok(true)
}

/// Complete path replacement, calculated after earlier commands are flushed.
pub(crate) struct PreparedMonsterMovement {
    pub transform: WorldTransform,
    pub movement: WorldMovementState,
    pub spline: Option<MovementSpline>,
}

/// Resolves native path admission independently of packet/frame storage borrows.
pub(crate) fn prepare_monster_move(
    current: WorldTransform,
    movement: WorldMovementState,
    message: &MonsterMove,
    receipt_ms: u32,
    stop_distance_tolerance: f32,
    target_position: impl FnOnce(u64) -> Option<Vec3>,
) -> Result<PreparedMonsterMovement, MovementSplineError> {
    let facing = facing(message.facing);
    let request = match &message.path {
        Some(path) => MovementPathRequest::Move {
            start: Vec3::from_array(message.start),
            points: path.points.iter().copied().map(Vec3::from_array).collect(),
            flags: path.flags,
            duration_ms: path.duration_ms,
        },
        None => MovementPathRequest::Stop {
            destination: Vec3::from_array(message.start),
        },
    };
    let prepared = request
        .prepare(current, movement.speeds().run(), stop_distance_tolerance)
        .map_err(MovementSplineError::from)?;
    let mut context = movement.context();
    context.transport = None;
    let mut movement_flags = movement.flags() & !0x0140_0000_0000;
    if message.control_byte != 0 {
        movement_flags |= 0x0040_0000_0000;
    }
    // 006ED7E0 resets ordinary movement when no active spline owns it.
    if movement.spline().is_none_or(|path| path.flags & 0x400 != 0) {
        movement_flags = reset_movement_flags(movement_flags);
    }
    let prepared = match prepared {
        PreparedMovementPath::Spline {
            destination, flags, ..
        } if flags & 0x1800000 != 0 || movement_flags & 0x100800 != 0 && flags & 0xa00 == 0 => {
            // Native 006EB680 refuses these path admissions and its caller
            // enters the immediate endpoint/facing path instead.
            PreparedMovementPath::Place { destination, flags }
        }
        prepared => prepared,
    };
    match prepared {
        PreparedMovementPath::Place { destination, .. } => {
            let transform = WorldTransform::new(destination, current.orientation());
            let orientation = facing.resolve(transform, target_position);
            movement_flags = reset_movement_flags(movement_flags) & !0x0080_0000_0000;
            Ok(PreparedMonsterMovement {
                transform: WorldTransform::new(destination, orientation),
                movement: WorldMovementState::new(movement_flags, movement.speeds(), context),
                spline: None,
            })
        }
        PreparedMovementPath::Spline {
            controls,
            destination,
            duration_ms,
            mut flags,
        } => {
            flags |= match message.facing {
                WireFacing::Point(_) => 0x8000,
                WireFacing::Target(_) => 0x10000,
                WireFacing::Angle(_) => 0x20000,
                WireFacing::Direction => 0,
            };
            // 0098C770 -> 00988A20 forces forward/backward while clearing
            // pending forward transitions. Walking is selected from path speed.
            movement_flags &= !0x34003;
            movement_flags |= if flags & 0x8000000 == 0 { 1 } else { 2 };
            if flags & 0xa00 != 0 && movement_flags & 0x800 != 0 {
                movement_flags = (movement_flags & !0x800) | 0x100000;
            }
            if flags & 0x200 != 0 && movement_flags & 0x2000000 != 0 {
                movement_flags &= !0x02c000c0;
            }
            let parabolic = message.path.as_ref().and_then(|path| path.parabolic);
            let animation = message.path.as_ref().and_then(|path| path.animation);
            let (vertical_acceleration, effect_start_ms) =
                if let Some((acceleration, delay)) = parabolic {
                    movement_flags &= !0x0080_0000_0000;
                    if delay == 0 {
                        movement_flags |= 0x0080_0000_0000;
                    }
                    (acceleration, delay)
                } else if let Some((tier, delay)) = animation {
                    flags = (flags & !0xff) | u32::from(tier);
                    if delay == 0 {
                        movement_flags |= 0x0100_0000_0000;
                    }
                    (0.0, delay)
                } else {
                    (0.0, 0)
                };
            let spline = MovementSpline::new(
                MovementSplineDefinition {
                    flags,
                    facing,
                    id: message.id,
                    elapsed_ms: 0,
                    duration_ms,
                    duration_scale: 1.0,
                    next_duration_scale: 1.0,
                    vertical_acceleration,
                    effect_start_ms,
                    nodes: controls,
                    destination,
                },
                receipt_ms,
                current,
            )?;
            let summary = spline.motion();
            Ok(PreparedMonsterMovement {
                transform: current,
                movement: WorldMovementState::new(movement_flags, movement.speeds(), context)
                    .with_spline(summary),
                spline: Some(spline),
            })
        }
    }
}

/// Native 006E9980/006ED7E0 release all axes while retaining walking/effects.
fn reset_movement_flags(mut flags: u64) -> u64 {
    if flags & 0x1000 != 0 {
        flags &= !0x3000;
    }
    if flags & 0x100000 != 0 {
        flags = (flags & !0x00dfc0ff) | 0x800;
    }
    flags & !0x1c00_04cf_f0ff
}

/// Converts only the final-facing vocabulary, without depending on wire storage.
fn facing(source: WireFacing) -> MovementSplineFacing {
    match source {
        WireFacing::Direction => MovementSplineFacing::Direction,
        WireFacing::Point(point) => MovementSplineFacing::Point(Vec3::from_array(point)),
        WireFacing::Target(guid) => MovementSplineFacing::Target(guid),
        WireFacing::Angle(angle) => MovementSplineFacing::Angle(angle),
    }
}
