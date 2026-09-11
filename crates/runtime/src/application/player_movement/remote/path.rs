//! Forced path-parent admission and local spline evaluation before world projection.

use glam::Vec3;
use solarity_ecs::WorldTransform;
use solarity_network::MonsterMove;
use solarity_systems::MovementGroundProfile;

use super::super::spline::{SplineStep, prepare_passenger_path, project};
use super::super::{LocalMovementGeometry, RuntimePlayerMovementError};
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
        let Some(prepared) = prepare_passenger_path(
            (world, movement),
            old_parent,
            message,
            receipt_ms,
            stop_distance_tolerance,
            geometry,
            target_position,
        )?
        else {
            return Ok(());
        };
        let (world, movement, parent) = (prepared.world, prepared.movement, prepared.parent);
        self.baseline_with_passenger(
            world,
            movement,
            receipt_ms,
            parent.map(|parent| parent.identity),
            true,
        );
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
        motion.remote_profile = Some(profile);
        motion.apply_spline_step(
            SplineStep {
                current,
                local,
                movement,
                summary,
                duration,
                scene_collision: self.scene_collision,
            },
            dimensions,
            geometry,
        )?;
        motion.time_ms = now_ms;
        self.ground_normal = motion.ground_normal;
        self.path_parent = motion.passenger;
        self.published = self.snapshot();
        Ok(())
    }
}
