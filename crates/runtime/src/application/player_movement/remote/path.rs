//! Forced path-parent admission and local spline evaluation before world projection.

use glam::Vec3;
use solarity_ecs::{WorldMovementState, WorldMovementTransport, WorldTransform};
use solarity_network::MonsterMove;
use solarity_systems::{
    MovementFallState, MovementFallTrajectory, MovementGroundProfile, MovementSplineTarget,
};

use super::super::passenger::PassengerParent;
use super::super::{
    LocalMovement, LocalMovementGeometry, MovementPhase, RuntimePlayerMovementError,
};
use super::state::RemoteUnit;

impl RemoteUnit {
    /// 73C8E0 flushes earlier snapshots, changes the parent, and then constructs
    /// the path. An unresolved nonzero parent leaves the former link untouched.
    pub(super) fn receive_path<G: LocalMovementGeometry>(
        &mut self,
        message: &MonsterMove,
        receipt_ms: u32,
        stop_distance_tolerance: f32,
        geometry: &mut G,
        target_position: impl Fn(u64) -> Option<Vec3>,
    ) -> Result<(), RuntimePlayerMovementError> {
        self.initialize_motion(geometry)?;
        self.flush(geometry)?;
        let (world, movement) = self.snapshot();
        let requested = message.transport.filter(|parent| parent.guid != 0);
        let old_parent = self
            .motion
            .as_ref()
            .and_then(|motion| motion.passenger)
            .or(self.path_parent);
        let stopped = crate::application::gameplay_session::stop_before_path(movement);
        if stopped != movement {
            if let Some(motion) = &mut self.motion {
                self.animation_events.append(&mut motion.animation_events);
                self.ground_normal = motion.ground_normal;
            }
            self.motion = None;
            self.published = (world, stopped);
        }
        let movement = stopped;
        let parent = if let Some(requested) = requested {
            if let Some(parent) =
                old_parent.filter(|parent| parent.identity.guid() == requested.guid)
            {
                Some(parent)
            } else {
                let Some(parent) = geometry.passenger(requested.guid)? else {
                    // 6EC400 rejects before unlinking; 73C8E0's GUID comparison
                    // then returns without admitting any replacement geometry.
                    return Ok(());
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
        self.baseline(world, movement, receipt_ms);
        self.path = prepared.spline;
        self.path_parent = parent.filter(|_| self.path.is_some());
        Ok(())
    }

    /// Frames change while local path controls and elapsed spline time stay fixed.
    pub(super) fn refresh_path_parent<G: LocalMovementGeometry>(
        &mut self,
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        let (world, movement) = self.published;
        let Some(transport) = movement.context().transport else {
            self.path_parent = None;
            return Ok(());
        };
        let previous = self.path_parent;
        self.path_parent = geometry
            .passenger(transport.guid)?
            .filter(|parent| previous.is_none_or(|old| old.identity == parent.identity));
        let local = WorldTransform::new(transport.position, transport.orientation);
        if self.path_parent.is_some() {
            self.published = project(local, movement, self.path_parent, Some(transport));
        } else {
            // Initial unresolved admission retains the ordinary world snapshot
            // (9872C0); a lost existing link leaves the local lane (6EC400).
            self.published = project(
                if previous.is_some() { local } else { world },
                movement,
                None,
                None,
            );
        }
        Ok(())
    }

    /// Samples once, applies the previous scene's collision admission, and keeps
    /// 6E9470's small ground corrections while the sampled path remains active.
    pub(super) fn advance_path<G: LocalMovementGeometry>(
        &mut self,
        now_ms: u32,
        dimensions: [f32; 3],
        profile: MovementGroundProfile,
        geometry: &mut G,
        target_position: impl Fn(u64) -> Option<Vec3>,
    ) -> Result<(), RuntimePlayerMovementError> {
        let (world, movement) = self.snapshot();
        let Some(path) = &mut self.path else {
            return Ok(());
        };
        let duration = now_ms.wrapping_sub(self.time_ms);
        if duration == 0 {
            return Ok(());
        }
        let transport = movement.context().transport;
        let current = transport.map_or(world, |parent| {
            WorldTransform::new(parent.position, parent.orientation)
        });
        let parent = self.path_parent;
        let (local, movement) = path.advance_movement(now_ms, movement, current, |guid| {
            target_position(guid)
                .map(|point| parent.map_or(point, |parent| parent.frame.local_position(point)))
        })?;
        let summary = path.motion();
        let Some(motion) = &mut self.motion else {
            // Spatial/hover paths retain their separate response owner.
            self.published = project(local, movement, parent, transport);
            return Ok(());
        };
        motion.published = project(local, movement, parent, transport);
        motion.orientation = local.orientation();
        motion.flags = movement.flags() as u32 & 0x77ff_fdff;
        motion.secondary = (movement.flags() >> 32) as u16;
        motion.remote_profile = Some(profile);
        // 6EAC40 advances the owner clock before 6E9C30 can snap and return.
        motion.elapsed_ms = motion.elapsed_ms.wrapping_add(duration);
        let target = MovementSplineTarget::new(current.position(), local.position(), duration)?;
        if summary.flags & 0x400 != 0 {
            // 98CA00 finalizes placement and returns before collision sampling.
            motion.position = local.position();
            motion.stop_path_fall()?;
            motion.reanchor()?;
        } else if target.requires_snap(motion.flags) {
            // 6E9C30 rebases launch height without ending an ordinary fall.
            motion.position = local.position();
            if let MovementPhase::Fall(fall) = motion.phase {
                let mut fall = fall.snapshot();
                fall.position = motion.position;
                fall.launch_height = motion.position.z
                    + MovementFallTrajectory::new(fall.mode, fall.initial_downward_speed)?
                        .distance_at_millis(fall.fall_time_ms)?;
                motion.retained_launch_height = fall.launch_height;
                motion.phase = MovementPhase::Fall(MovementFallState::new(fall)?);
            }
        } else if !self.scene_collision {
            // 6E9E20 -> 988490 clears ordinary falling outside scene admission.
            motion.stop_path_fall()?;
            motion.elapsed_ms = motion.elapsed_ms.wrapping_add(duration);
            motion.position = local.position();
        } else {
            motion.spline_interval(local.position(), duration, summary, dimensions, geometry)?;
            let corrected = target.corrected_position(motion.position)?;
            if corrected != motion.position {
                // 6E9470 -> 9886E0 also clears ordinary falling and resets its anchor.
                motion.position = corrected;
                motion.stop_path_fall()?;
                motion.reanchor()?;
            }
        }
        motion.time_ms = now_ms;
        self.ground_normal = motion.ground_normal;
        self.path_parent = motion.passenger;
        self.published = self.snapshot();
        Ok(())
    }
}

/// The ECS transform is always world space; its conditional transport pose is local.
fn project(
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
    fn stop_path_fall(&mut self) -> Result<(), RuntimePlayerMovementError> {
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
