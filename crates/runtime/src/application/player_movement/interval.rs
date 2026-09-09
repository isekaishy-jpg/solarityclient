//! Shared collision continuation for analytic movement and fixed spline travel.

use std::collections::VecDeque;

use glam::Vec3;
use solarity_ecs::WorldTransform;
use solarity_network::WorldMovementKind;
use solarity_systems::{MovementIntervalDrive, MovementSplineTarget, MovementTravelAxes};

use super::{
    LocalMovement, LocalMovementGeometry, MovementFallAdmission, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallPhase, MovementFallTrajectory,
    MovementGroundContinuation, MovementGroundInterval, MovementGroundProfile,
    MovementGroundSnapshot, MovementGroundState, MovementGroundTrajectory, MovementIntervalMode,
    MovementIntervalRequest, MovementPhase, MovementSupportProfile, PlayerMovementOutput,
    RuntimePlayerMovementError, UnitMovementAnimationEventKind, fall_mode, swimming,
};

/// Native 762E00 retains these inputs across ground/fall response substeps.
#[derive(Clone, Copy)]
struct SplineInterval {
    travel: MovementIntervalDrive,
    target: Vec3,
    ground_speed: f32,
    refresh_ground_normal: bool,
}

impl LocalMovement {
    /// Advances the ordinary analytic trajectory through the shared collision owner.
    pub(super) fn interval<G: LocalMovementGeometry>(
        &mut self,
        duration: u32,
        dimensions: [f32; 3],
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        self.driven_interval(duration, None, dimensions, geometry, output)
    }

    /// 6E9E20 passes the float target delta into 762E00's fixed travel interval.
    /// Admission here is restricted to ordinary horizontal walking splines.
    pub(super) fn spline_interval<G: LocalMovementGeometry>(
        &mut self,
        target: Vec3,
        duration: u32,
        spline: solarity_ecs::WorldMovementSpline,
        dimensions: [f32; 3],
        geometry: &mut G,
    ) -> Result<(), RuntimePlayerMovementError> {
        let delta = target - self.position;
        let target = self.position + delta;
        // 762E00 validates the rounded target with radius + A37F94 before
        // collecting geometry. Rejected intervals preserve position and normal.
        let edge = f64::from(f32::from_bits(0x4685_5555))
            - f64::from(dimensions[0] + f32::from_bits(0x428e_c04e));
        if !target.is_finite()
            || f64::from(target.x).abs() >= edge
            || f64::from(target.y).abs() >= edge
        {
            return Ok(());
        }
        let source = SplineInterval {
            travel: MovementIntervalDrive::new(delta, duration, MovementTravelAxes::Horizontal)?,
            target,
            ground_speed: solarity_systems::resolve_unit_movement_speed(
                self.snapshot().1.with_spline(spline),
            ),
            refresh_ground_normal: spline.flags & 0x2000 == 0,
        };
        self.driven_interval(
            duration,
            Some(source),
            dimensions,
            geometry,
            &mut VecDeque::new(),
        )
    }

    /// Consumes collision time while preserving the selected source's travel rules.
    fn driven_interval<G: LocalMovementGeometry>(
        &mut self,
        duration: u32,
        source: Option<SplineInterval>,
        dimensions: [f32; 3],
        geometry: &mut G,
        output: &mut VecDeque<PlayerMovementOutput>,
    ) -> Result<(), RuntimePlayerMovementError> {
        if !self.active {
            return Ok(());
        }
        if self.flags & 0x800 != 0 {
            self.heartbeat_ms = self.heartbeat_ms.wrapping_add(duration);
            return Ok(());
        }
        if self.flags & 0xc010ff == 0 {
            return Ok(());
        }
        let [radius, height, step_height] = dimensions;
        let profile = self
            .remote_profile
            .unwrap_or(MovementGroundProfile::PlayerControlled { step_height });
        let mut remaining = duration;
        while remaining != 0 {
            if source.is_none() {
                self.elapsed_ms = self.elapsed_ms.wrapping_add(remaining);
            }
            let swimming = matches!(self.phase, MovementPhase::Swimming(_));
            let (delta, blended_position, distance, direction3) = if let Some(source) = source {
                (
                    Vec3::ZERO,
                    false,
                    source.travel.distance(remaining),
                    source.travel.direction(),
                )
            } else {
                let sample = self.ground.sample(self.elapsed_ms);
                self.orientation = self.yaw.sample(self.elapsed_ms);
                let mut delta = match self.phase {
                    MovementPhase::Swimming(trajectory) => {
                        let sample = trajectory.sample(self.elapsed_ms);
                        self.orientation = sample.orientation;
                        self.context.pitch_radians = Some(sample.pitch);
                        self.anchor + sample.displacement - self.position
                    }
                    MovementPhase::Ground { .. } => {
                        self.anchor + sample.displacement - self.position
                    }
                    MovementPhase::Fall(fall) => {
                        let state = fall.snapshot();
                        let seconds = (f64::from(self.elapsed_ms)
                            * f64::from(f32::from_bits(0x3a83_126f)))
                            as f32;
                        (self.anchor
                            + (state.direction.as_dvec3()
                                * f64::from(seconds)
                                * f64::from(state.horizontal_speed))
                            .as_vec3())
                            - self.position
                    }
                };
                let mut blended_position = false;
                if let Some(blend) = &mut self.blend {
                    if let MovementPhase::Fall(fall) = self.phase {
                        let fall = fall.snapshot();
                        delta.z =
                            MovementFallTrajectory::new(fall.mode, fall.initial_downward_speed)?
                                .vertical_displacement(
                                    fall.fall_time_ms.wrapping_add(remaining),
                                    self.position.z,
                                    fall.launch_height,
                                )?;
                    }
                    let mut analytic = solarity_systems::RemoteMovementPose {
                        transform: WorldTransform::new(self.position + delta, self.orientation),
                        pitch: self.context.pitch_radians.unwrap_or(0.0),
                        transport_guid: self.passenger.map_or(0, |parent| parent.identity.guid()),
                    };
                    blended_position = blend.sample(
                        self.position,
                        self.time_ms.wrapping_add(duration - remaining),
                        remaining,
                        &mut analytic,
                    );
                    delta = analytic.transform.position() - self.position;
                    self.orientation = analytic.transform.orientation();
                    if self.context.pitch_radians.is_some() {
                        self.context.pitch_radians = Some(analytic.pitch);
                    }
                }
                let distance = if swimming {
                    delta.as_dvec3().length() as f32
                } else {
                    delta.truncate().as_dvec2().length() as f32
                };
                let direction3 = if distance.abs() >= f32::from_bits(0x3580_0000) {
                    (delta.as_dvec3() * (1.0 / f64::from(distance))).as_vec3()
                } else {
                    Vec3::ZERO
                };
                (delta, blended_position, distance, direction3)
            };
            if self.flags & 0xc0100f == 0 {
                return Ok(());
            }
            let direction = direction3.truncate();
            let mode = match self.phase {
                MovementPhase::Swimming(_) => MovementIntervalMode::SwimmingOrFlying,
                MovementPhase::Ground { .. } => MovementIntervalMode::Grounded(profile),
                MovementPhase::Fall(fall) => {
                    let fall = fall.snapshot();
                    MovementIntervalMode::Airborne {
                        fall_time_ms: fall.fall_time_ms,
                        launch_height: fall.launch_height,
                        trajectory: MovementFallTrajectory::new(
                            fall.mode,
                            fall.initial_downward_speed,
                        )?,
                    }
                }
            };
            let request = MovementIntervalRequest {
                position: self.position,
                radius,
                height,
                distance,
                direction: if swimming {
                    direction3
                } else {
                    direction.extend(0.)
                },
                duration_ms: remaining,
                mode,
            };
            let ready = if swimming {
                geometry.collect_swimming(request)?
            } else {
                geometry.collect(request)?
            };
            if !ready {
                if let Some(source) = source {
                    // 762E00 -> 75D3C0 stores the initial target and its travel
                    // normal when a path cannot collect its first geometry.
                    self.ground_normal =
                        MovementSplineTarget::new(self.position, source.target, duration)?
                            .unavailable_geometry_normal();
                    self.position = source.target;
                    if let MovementPhase::Fall(fall) = self.phase {
                        let mut fall = fall.snapshot();
                        fall.position = self.position;
                        self.phase = MovementPhase::Fall(super::MovementFallState::new(fall)?);
                    }
                } else {
                    self.elapsed_ms = self.elapsed_ms.wrapping_sub(remaining);
                    self.skip(remaining, output);
                    self.update_ground_normal(dimensions, geometry)?;
                }
                return Ok(());
            }
            // 7618B0 and 760B40 check parent retention before fall/swim motion.
            // A leave consumes no collision time and terminates this interval.
            if matches!(
                self.phase,
                MovementPhase::Fall(_) | MovementPhase::Swimming(_)
            ) && self.passenger.is_some()
                && self.contact_passenger(0, geometry)?
            {
                self.elapsed_ms = self.elapsed_ms.saturating_sub(remaining);
                let saved = self.time_ms;
                self.time_ms = saved.wrapping_add(duration - remaining);
                self.emit(WorldMovementKind::ChangeTransport, output)?;
                self.time_ms = saved;
                if source.is_none_or(|source| source.refresh_ground_normal) {
                    self.update_ground_normal(dimensions, geometry)?;
                }
                return Ok(());
            }
            let was_airborne = matches!(self.phase, MovementPhase::Fall(_));
            let mut landing = None;
            let mut surface_jump = false;
            let (consumed, reset, skipped, contact) = match self.phase {
                MovementPhase::Swimming(_) => {
                    let advance = swimming::advance(
                        solarity_systems::MovementSwimInterval {
                            position: self.position,
                            radius,
                            height,
                            duration_ms: remaining,
                            distance,
                            direction: direction3,
                            ascending: self.flags & 0x400000 != 0,
                        },
                        geometry,
                    )?;
                    self.position = advance.position;
                    if advance.attempt_surface_jump && self.can_surface_jump() {
                        self.launch_swim_jump()?;
                        surface_jump = true;
                    }
                    (advance.consumed_ms, advance.reset_motion_anchor, 0, None)
                }
                MovementPhase::Ground { step_anchor } => {
                    let current_basis = MovementGroundTrajectory::new(
                        self.flags,
                        self.secondary & 8 != 0,
                        self.orientation,
                        self.speeds,
                    )?;
                    let state = MovementGroundState::new(MovementGroundSnapshot {
                        position: self.position,
                        step_anchor,
                        fall_time_ms: self.context.fall_time_ms,
                        launch_height: self.retained_launch_height,
                        initial_downward_speed: self.retained_downward_speed,
                        horizontal_direction: current_basis.direction(),
                        // 987570 stores active path length/duration in the
                        // movement owner before a ground contact can launch it.
                        horizontal_speed: source
                            .map_or(self.ground.speed(), |source| source.ground_speed),
                        direction: current_basis.direction().extend(0.),
                        mode: fall_mode(self.flags),
                        fall_admission: if self.flags & 0x2201e00 == 0 && self.secondary & 4 == 0 {
                            MovementFallAdmission::Allowed
                        } else {
                            MovementFallAdmission::Suppressed
                        },
                    })?;
                    let result = state.advance_with_geometry(
                        MovementGroundInterval {
                            duration_ms: remaining,
                            distance,
                            direction,
                            radius,
                            height,
                            profile,
                        },
                        geometry,
                    )?;
                    match result.continuation {
                        MovementGroundContinuation::Grounded(state) => {
                            let state = state.snapshot();
                            self.position = state.position;
                            self.context.fall_time_ms = state.fall_time_ms;
                            self.retained_launch_height = state.launch_height;
                            self.retained_downward_speed = state.initial_downward_speed;
                            self.phase = MovementPhase::Ground {
                                step_anchor: state.step_anchor,
                            };
                            if state.step_anchor.is_some() {
                                self.flags |= 0x0400_0000;
                            } else {
                                self.flags &= !0x0400_0000;
                            }
                        }
                        MovementGroundContinuation::Falling(fall) => {
                            self.position = fall.snapshot().position;
                            self.retained_launch_height = fall.snapshot().launch_height;
                            self.retained_downward_speed = fall.snapshot().initial_downward_speed;
                            self.phase = MovementPhase::Fall(fall);
                            self.flags = self.flags & 0xf91f_ff3f | 0x1000;
                        }
                    }
                    (
                        result.consumed_ms,
                        result.reset_motion_anchor,
                        result.skipped_time_ms,
                        result.contact_triangle,
                    )
                }
                MovementPhase::Fall(fall) => {
                    let state = fall.snapshot();
                    let curve =
                        MovementFallTrajectory::new(state.mode, state.initial_downward_speed)?;
                    let vertical = curve.vertical_displacement(
                        state.fall_time_ms.wrapping_add(remaining),
                        self.position.z,
                        state.launch_height,
                    )?;
                    let displacement = (direction * distance).extend(if blended_position {
                        delta.z
                    } else {
                        vertical
                    });
                    let result = fall.advance_with_geometry(
                        MovementFallInterval {
                            duration_ms: remaining,
                            displacement,
                            radius,
                            height,
                            support_profile: match profile {
                                MovementGroundProfile::PlayerControlled { .. } => {
                                    MovementSupportProfile::PlayerControlled
                                }
                                MovementGroundProfile::Other => MovementSupportProfile::Other,
                            },
                            policy: if self.flags & 0xf == 0 {
                                MovementFallAdvancePolicy::Live
                            } else {
                                MovementFallAdvancePolicy::LiveTranslating
                            },
                        },
                        geometry,
                    )?;
                    match result.continuation {
                        MovementFallContinuation::Airborne(fall) => {
                            self.position = fall.snapshot().position;
                            self.retained_launch_height = fall.snapshot().launch_height;
                            self.retained_downward_speed = fall.snapshot().initial_downward_speed;
                            self.phase = MovementPhase::Fall(fall);
                            if fall.snapshot().phase == MovementFallPhase::FallingFar {
                                self.flags |= 0x2000;
                            }
                        }
                        MovementFallContinuation::Landed {
                            position,
                            fall_time_ms,
                        } => {
                            let previous_flags = self.flags;
                            self.initial_contact_pending = false;
                            self.position = position;
                            self.context.fall_time_ms = fall_time_ms;
                            self.phase = MovementPhase::Ground { step_anchor: None };
                            self.flags &= !0x3000;
                            self.apply_deferred();
                            self.reanchor()?;
                            landing = Some(UnitMovementAnimationEventKind::Land {
                                previous_flags,
                                forced: state.initial_downward_speed != 0.0,
                                slow: source
                                    .map_or(self.ground.speed(), |source| source.ground_speed)
                                    <= self.speeds.walk() * 2.0,
                            });
                        }
                    }
                    (
                        result.consumed_ms,
                        result.reset_motion_anchor,
                        result.skipped_time_ms,
                        result.contact_triangle,
                    )
                }
            };
            geometry.check_failure()?;
            if !was_airborne && (reset || blended_position) {
                self.reanchor()?;
            }
            let contact_guid = geometry.contact_guid(contact);
            // 7620F0 reports even a static zero GUID; 7612B0 only reports a
            // nonzero parent after its airborne contact response.
            let changed = contact.is_some()
                && (!was_airborne || contact_guid != 0)
                && self.contact_passenger(contact_guid, geometry)?;
            if was_airborne && (reset || blended_position) {
                self.reanchor()?;
            }
            if !(reset || blended_position) {
                self.elapsed_ms = self.elapsed_ms.wrapping_sub(skipped);
            }
            let saved = self.time_ms;
            self.time_ms = saved.wrapping_add(duration - remaining + consumed);
            // 6EB0B0 gives the landing packet precedence over ChangeTransport.
            if let Some(landing) = landing {
                self.notify_animation(landing);
                self.emit(WorldMovementKind::FallLand, output)?;
            } else if changed {
                self.emit(WorldMovementKind::ChangeTransport, output)?;
            } else if surface_jump {
                self.notify_animation(UnitMovementAnimationEventKind::Changed);
                self.emit(WorldMovementKind::Jump, output)?;
            }
            self.time_ms = saved;
            self.skip(skipped, output);
            remaining = remaining.saturating_sub(consumed);
            if changed || landing.is_some() {
                self.elapsed_ms = self.elapsed_ms.saturating_sub(remaining);
                if source.is_none_or(|source| source.refresh_ground_normal) {
                    self.update_ground_normal(dimensions, geometry)?;
                }
                return Ok(());
            }
            if source.is_some() && self.elapsed_ms == 0 && self.flags & 0xf != 0 {
                // 7630E0 restores only the unconsumed clock after an anchor reset.
                self.elapsed_ms = remaining;
            }
        }
        if source.is_none_or(|source| source.refresh_ground_normal) {
            self.update_ground_normal(dimensions, geometry)?;
        }
        Ok(())
    }
}
