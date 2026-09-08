//! Unit immersion commands and the local 3D movement/collision boundary.

use std::collections::VecDeque;

use glam::Vec3;
use solarity_ecs::ActiveWorld;
use solarity_network::WorldMovementKind;
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementFallPhase, MovementFallSnapshot,
    MovementFallState, MovementGeometry, MovementSwimAdvance, MovementSwimGeometry,
    MovementSwimImmersion, MovementSwimInterval, MovementSwimTrajectory, MovementSwimTransition,
    MovementTransportChange, SubmergedLiquid,
};

use super::{
    LocalMovement, LocalMovementGeometry, MovementCommand, MovementPhase, PlayerMovementOutput,
    RuntimePlayerMovementError, fall_mode,
};
use crate::application::unit_animation::UnitMovementAnimationEventKind;

/// Adapts the local world's ordinary and water providers without copying faces.
struct SwimGeometry<'a, G>(&'a mut G);

impl<G: LocalMovementGeometry> MovementGeometry for SwimGeometry<'_, G> {
    type TriangleIdentity = G::TriangleIdentity;
    fn prepare_sweep(
        &mut self,
        volume: &MovementCollisionVolume,
        direction: Vec3,
        distance: f32,
    ) -> bool {
        self.0.prepare_sweep(volume, direction, distance)
    }
    fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.0.triangles()
    }
    fn triangle_identity(&self, index: usize) -> Option<Self::TriangleIdentity> {
        self.0.triangle_identity(index)
    }
}

impl<G: LocalMovementGeometry> MovementSwimGeometry for SwimGeometry<'_, G> {
    fn water_triangles(&self) -> &[MovementCollisionTriangle] {
        self.0.water_triangles()
    }
}

/// Runs native swimming response over the current local geometry borrow.
pub(super) fn advance<G: LocalMovementGeometry>(
    interval: MovementSwimInterval,
    geometry: &mut G,
) -> Result<MovementSwimAdvance, RuntimePlayerMovementError> {
    Ok(interval.advance(&mut SwimGeometry(geometry))?)
}

impl LocalMovement {
    /// Resolves the current 3D basis at a native reanchor boundary.
    pub(super) fn swim_trajectory(
        &self,
    ) -> Result<MovementSwimTrajectory, RuntimePlayerMovementError> {
        Ok(MovementSwimTrajectory::new(
            self.flags,
            self.secondary,
            self.orientation,
            self.context.pitch_radians.unwrap_or(0.0),
            self.speeds,
        )?)
    }

    /// 73AB20 calls 730D10 after position registration, queuing same-time events
    /// for the next movement command dispatch instead of changing flags inline.
    pub(super) fn queue_immersion(
        &mut self,
        liquid: Option<SubmergedLiquid>,
        height: f32,
        world: &ActiveWorld,
        commands: &mut VecDeque<MovementCommand>,
    ) -> Result<bool, RuntimePlayerMovementError> {
        let Some(update) = (MovementSwimImmersion {
            flags: self.flags,
            secondary: self.secondary,
            unit_flags: world
                .unit_flags(self.identity.guid())
                .unwrap_or_default()
                .primary(),
            parent_guid: self.passenger.map_or(0, |parent| parent.identity.guid()),
            locally_controlled: self.active && self.client_control,
            height,
            liquid_depth: liquid.map(|sample| sample.depth),
            previous_depth: self.previous_water_depth,
            fall_time_ms: match self.phase {
                MovementPhase::Fall(fall) => fall.snapshot().fall_time_ms,
                _ => self.context.fall_time_ms,
            },
            initial_downward_speed: self.retained_downward_speed,
        })
        .evaluate()?
        else {
            return Ok(false);
        };
        self.previous_water_depth = update.previous_depth;
        self.is_swimming = update.is_swimming;
        if update.splash {
            self.water_splashes
                .push_back(super::super::unit_water::UnitWaterSplash {
                    identity: self.identity,
                    position: self.world_position(),
                });
        }
        // 6EC090 inserts after equal timestamps and before future commands.
        // A queued input from a later platform tick must not delay immersion.
        let mut index = commands
            .iter()
            .position(|command| (self.time_ms.wrapping_sub(command.timestamp_ms()) as i32) < 0)
            .unwrap_or(commands.len());
        if update.attempt_surface_jump {
            commands.insert(
                index,
                MovementCommand::SurfaceJump {
                    timestamp_ms: self.time_ms,
                },
            );
            index += 1;
        }
        if let Some(transition) = update.transition {
            commands.insert(
                index,
                MovementCommand::Swim {
                    transition,
                    timestamp_ms: self.time_ms,
                },
            );
            // 730DE8 -> 721210 queues the swim command, then discovers the
            // swimming tutorial only for the selected player's full GUID.
            if transition == MovementSwimTransition::Enter
                && world.local_player_guid()? == self.identity.guid()
            {
                self.tutorials.push_back(27);
            }
        }
        Ok(update.splash)
    }

    /// Executes deferred native events 15/16 and freezes their wire snapshots.
    pub(super) fn swim_transition(
        &mut self,
        transition: MovementSwimTransition,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        let kind = match transition {
            MovementSwimTransition::Enter => {
                if let MovementPhase::Fall(fall) = self.phase {
                    self.context.fall_time_ms = fall.snapshot().fall_time_ms;
                }
                self.flags = (self.flags & 0xf9ff_ffff | 0x200000) & !0x3000;
                self.phase = MovementPhase::Swimming(self.swim_trajectory()?);
                self.initial_contact_pending = false;
                self.apply_deferred();
                self.reanchor()?;
                WorldMovementKind::StartSwim
            }
            MovementSwimTransition::Leave => {
                let retained_speed = match self.phase {
                    MovementPhase::Swimming(trajectory) => trajectory.speed(),
                    MovementPhase::Fall(fall) => fall.snapshot().horizontal_speed,
                    MovementPhase::Ground { .. } => self.ground.speed(),
                };
                self.flags &= 0xff1f_ffff;
                if self.secondary & 0x20 == 0 {
                    self.flags &= !0xc0;
                    self.context.pitch_radians = Some(0.0);
                }
                self.phase = MovementPhase::Ground { step_anchor: None };
                self.reanchor()?;
                // 98BFF0 clears swimming before 988370 refreshes the basis,
                // while +8C still holds the former swim speed for this launch.
                if self.flags & 0x2201e00 == 0 && self.secondary & 4 == 0 {
                    self.start_swim_fall(0.0, self.ground.travel_direction(), retained_speed)?;
                }
                WorldMovementKind::StopSwim
            }
        };
        self.notify_animation(UnitMovementAnimationEventKind::Changed);
        self.emit(kind, output)
    }

    /// 9883F0's ordinary nonspline surface-jump admission.
    pub(super) fn can_surface_jump(&self) -> bool {
        self.flags & 0x2001800 == 0
            && self.secondary & 2 == 0
            && (self.secondary & 4 != 0 || self.flags & 0x400 == 0)
    }

    /// Water jumps retain the current pitched basis and native -9.096748 launch.
    pub(super) fn launch_swim_jump(&mut self) -> Result<(), RuntimePlayerMovementError> {
        let trajectory = self.swim_trajectory()?;
        self.start_swim_fall(
            f32::from_bits(0xc111_8c48),
            trajectory.direction(),
            trajectory.speed(),
        )
    }

    /// 988370 anchors before changing flags, preserving the selected launch basis.
    fn start_swim_fall(
        &mut self,
        downward: f32,
        direction: Vec3,
        speed: f32,
    ) -> Result<(), RuntimePlayerMovementError> {
        self.flags = self.flags & 0xf91f_ffff | 0x1000;
        if self.secondary & 0x20 == 0 {
            self.flags &= !0xc0;
        }
        self.retained_launch_height = self.position.z;
        self.retained_downward_speed = downward;
        self.phase = MovementPhase::Fall(MovementFallState::new(MovementFallSnapshot {
            position: self.position,
            fall_time_ms: 0,
            launch_height: self.position.z,
            initial_downward_speed: downward,
            horizontal_direction: MovementTransportChange::horizontal_direction(direction),
            horizontal_speed: speed,
            direction,
            mode: fall_mode(self.flags),
            phase: MovementFallPhase::Falling,
        })?);
        self.reanchor()
    }
}

#[cfg(test)]
#[path = "../../../tests/application/player_swimming.rs"]
mod tests;
