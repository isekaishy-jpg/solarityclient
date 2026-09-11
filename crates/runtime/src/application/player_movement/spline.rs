//! Shared server-path parent admission and ground collision response.

use super::passenger::PassengerParent;
use super::{LocalMovement, LocalMovementGeometry, MovementPhase, RuntimePlayerMovementError};
use glam::Vec3;
use solarity_ecs::{
    WorldMovementSpline, WorldMovementState, WorldMovementTransport, WorldTransform,
};
use solarity_network::MonsterMove;
use solarity_systems::{
    MovementFallState, MovementFallTrajectory, MovementSpline, MovementSplineTarget,
};

pub(super) struct PreparedPassengerPath {
    pub attachment: (WorldTransform, WorldMovementState),
    pub world: WorldTransform,
    pub movement: WorldMovementState,
    pub parent: Option<PassengerParent>,
    pub spline: Option<MovementSpline>,
}

pub(super) fn prepare_passenger_path<G: LocalMovementGeometry>(
    snapshot: (WorldTransform, WorldMovementState),
    old_parent: Option<PassengerParent>,
    message: &MonsterMove,
    receipt_ms: u32,
    stop_distance_tolerance: f32,
    geometry: &G,
    target_position: impl Fn(u64) -> Option<Vec3>,
) -> Result<Option<PreparedPassengerPath>, RuntimePlayerMovementError> {
    let (world, movement) = snapshot;
    let requested = message.transport.filter(|parent| parent.guid != 0);
    let parent = if let Some(requested) = requested {
        if let Some(parent) = old_parent.filter(|parent| parent.identity.guid() == requested.guid) {
            Some(parent)
        } else {
            let Some(parent) = geometry.passenger(requested.guid)? else {
                // 6EC400 rejects before unlinking; 73C8E0's GUID comparison
                // then returns without admitting any replacement geometry.
                return Ok(None);
            };
            Some(parent)
        }
    } else {
        None
    };
    let mut local = parent.map_or(world, |parent| {
        WorldTransform::new(
            parent.frame.local_position(world.position()),
            parent.frame.local_orientation(world.orientation()),
        )
    });
    if let Some(transport) = movement.context().transport
        && parent.is_some_and(|parent| parent.identity.guid() == transport.guid)
    {
        // The same parent already owns this local lane. Avoid a rounded
        // world/local round trip when preparing a replacement path.
        local = WorldTransform::new(transport.position, transport.orientation);
    }
    let transport = requested.map(|requested| {
        let retained = movement
            .context()
            .transport
            .filter(|old| old.guid == requested.guid);
        WorldMovementTransport {
            guid: requested.guid,
            seat: requested.seat,
            position: local.position(),
            orientation: local.orientation(),
            // MonsterMove supplies no transport clocks. Only same-parent
            // received metadata survives; a new remote link has no wire clock.
            time_ms: retained.map_or(0, |old| old.time_ms),
            interpolated_time_ms: retained.and_then(|old| old.interpolated_time_ms),
        }
    });
    let attachment = project(local, movement, parent, transport);
    let prepared = crate::application::gameplay_session::prepare_monster_move(
        local,
        movement,
        message,
        receipt_ms,
        stop_distance_tolerance,
        |guid| {
            target_position(guid)
                .map(|point| parent.map_or(point, |parent| parent.frame.local_position(point)))
        },
    )?;
    let (world, movement) = project(prepared.transform, prepared.movement, parent, transport);
    Ok(Some(PreparedPassengerPath {
        attachment,
        world,
        movement,
        parent,
        spline: prepared.spline,
    }))
}

pub(super) struct SplineStep {
    pub current: WorldTransform,
    pub local: WorldTransform,
    pub movement: WorldMovementState,
    pub summary: WorldMovementSpline,
    pub duration: u32,
    pub scene_collision: bool,
}

impl LocalMovement {
    pub(super) fn apply_spline_step<G: LocalMovementGeometry>(
        &mut self,
        step: SplineStep,
        dimensions: [f32; 3],
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        let SplineStep {
            current,
            local,
            movement,
            summary,
            duration,
            ..
        } = step;
        self.orientation = local.orientation();
        self.flags = movement.flags() as u32 & 0x77ff_fdff;
        self.secondary = (movement.flags() >> 32) as u16;
        // 6EAC40 advances the owner clock before 6E9C30 can snap and return.
        self.elapsed_ms = self.elapsed_ms.wrapping_add(duration);
        let target = MovementSplineTarget::new(current.position(), local.position(), duration)?;
        if summary.flags & 0x400 != 0 {
            // 98CA00 finalizes placement and returns before collision sampling.
            self.position = local.position();
            self.stop_path_fall()?;
            self.reanchor()?;
        } else if target.requires_snap(self.flags) {
            // 6E9C30 rebases launch height without ending an ordinary fall.
            self.position = local.position();
            if let MovementPhase::Fall(fall) = self.phase {
                let mut fall = fall.snapshot();
                fall.position = self.position;
                fall.launch_height = self.position.z
                    + MovementFallTrajectory::new(fall.mode, fall.initial_downward_speed)?
                        .distance_at_millis(fall.fall_time_ms)?;
                self.retained_launch_height = fall.launch_height;
                self.phase = MovementPhase::Fall(MovementFallState::new(fall)?);
            }
        } else if !step.scene_collision {
            // 6E9E20 -> 988490 clears ordinary falling outside scene admission.
            self.stop_path_fall()?;
            self.elapsed_ms = self.elapsed_ms.wrapping_add(duration);
            self.position = local.position();
        } else {
            self.spline_interval(local.position(), duration, summary, dimensions, geometry)?;
            let corrected = target.corrected_position(self.position)?;
            if corrected != self.position {
                // 6E9470 -> 9886E0 also clears ordinary falling and resets its anchor.
                self.position = corrected;
                self.stop_path_fall()?;
                self.reanchor()?;
            }
        }
        Ok(())
    }
}

/// The ECS transform is always world space; its conditional transport pose is local.
pub(super) fn project(
    local: WorldTransform,
    movement: WorldMovementState,
    parent: Option<PassengerParent>,
    transport: Option<WorldMovementTransport>,
) -> (WorldTransform, WorldMovementState) {
    let mut context = movement.context();
    context.transport = transport.map(|mut transport| {
        transport.position = local.position();
        transport.orientation = local.orientation();
        transport
    });
    let mut flags = movement.flags() & !(0x200 | (0x400_u64 << 32));
    if let Some(transport) = context.transport {
        flags |= 0x200;
        if transport.interpolated_time_ms.is_some() {
            flags |= 0x400_u64 << 32;
        }
    }
    let mut projected = WorldMovementState::new(flags, movement.speeds(), context);
    if let Some(spline) = movement.spline() {
        projected = projected.with_spline(spline);
    }
    let world = parent.map_or(local, |parent| {
        WorldTransform::new(
            parent.frame.world_position(local.position()),
            parent.frame.world_orientation(local.orientation()),
        )
    });
    (world, projected)
}

impl LocalMovement {
    /// 988490 releases ordinary fall state and restores a deferred root when needed.
    pub(super) fn stop_path_fall(&mut self) -> Result<(), RuntimePlayerMovementError> {
        if self.flags & 0x101000 == 0 {
            return Ok(());
        }
        if let MovementPhase::Fall(fall) = self.phase {
            self.context.fall_time_ms = fall.snapshot().fall_time_ms;
        }
        self.flags &= !0x3000;
        if self.flags & 0x100000 != 0 {
            self.flags = self.flags & 0xff20_3f00 | 0x800;
        }
        self.phase = MovementPhase::Ground {
            step_anchor: self.context.spline_elevation,
        };
        self.reanchor()
    }
}
