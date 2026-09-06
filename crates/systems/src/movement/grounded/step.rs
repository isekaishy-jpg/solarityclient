//! Native up/forward/down step probe and speculative falling admission.

use super::{
    DISTANCE_EPSILON, GroundQuery, MovementGroundAdvanceError, MovementGroundSnapshot, SLOPE,
    VECTOR_EPSILON,
};
use crate::movement::{
    MovementFallAdvancePolicy, MovementFallContinuation, MovementFallInterval,
    MovementFallTrajectory, MovementGeometry,
};
use glam::{Vec2, Vec3};

const CONTACT_TOLERANCE: f32 = f32::from_bits(0x3ab6_0b61);
const STEP_PROBE_PER_HEIGHT: f32 = f32::from_bits(0x3f98_8b62);
const TRIAL_DIRECTION_COSINE: f32 = f32::from_bits(0x3f7c_1c5c);

impl<G: MovementGeometry + ?Sized> GroundQuery<'_, G> {
    pub(super) fn step(
        &mut self,
        state: &mut MovementGroundSnapshot,
        incoming: Vec2,
        normal: Vec3,
    ) -> Result<bool, MovementGroundAdvanceError> {
        let original = state.position;
        let probe_distance = f64::from(self.interval.radius + CONTACT_TOLERANCE)
            .max(f64::from(self.interval.profile.step_height()) * f64::from(STEP_PROBE_PER_HEIGHT))
            as f32;
        let mut heading = incoming;
        if normal.z >= 0.0 && normal.z <= SLOPE {
            let reciprocal = 1.0 / normal.truncate().as_dvec2().length();
            let proposed = (-normal.truncate().as_dvec2() * reciprocal).as_vec2();
            let Some(probe) = self.sweep(state.position, proposed.extend(0.0), probe_distance)?
            else {
                return Ok(false);
            };
            if probe
                .last_triangle()
                .is_some_and(|index| self.geometry.triangles()[index].surface_normal() == normal)
            {
                heading = proposed;
            }
        }
        let Some(upward) =
            self.sweep(state.position, Vec3::Z, self.remaining_step(state) as f32)?
        else {
            return Ok(false);
        };
        let mut up_distance = upward.distance();
        let rise = state.step_anchor.map_or(f64::from(up_distance), |anchor| {
            f64::from(state.position.z) - f64::from(anchor) + f64::from(up_distance)
        });
        if rise.abs() < f64::from(VECTOR_EPSILON) {
            state.step_anchor = None;
            return Ok(true);
        }
        state.position.z += up_distance;
        let Some(mut forward) = self.sweep(state.position, heading.extend(0.0), probe_distance)?
        else {
            return Ok(false);
        };
        let mut horizontal_distance = forward.distance();
        state.position = (state.position.as_dvec3()
            + heading.extend(0.0).as_dvec3() * f64::from(horizontal_distance))
        .as_vec3();
        if (horizontal_distance - probe_distance).abs() >= DISTANCE_EPSILON
            && let Some(index) = forward.last_triangle()
            && self.classify(state, index)?.0
        {
            let mut remaining = probe_distance - horizontal_distance;
            let correction = self.follow_surface(
                state,
                heading.extend(0.0),
                &mut remaining,
                self.geometry.triangles()[index].surface_normal(),
                forward.combined_foot_normal(),
            );
            let mut delta = (heading.extend(0.0).as_dvec3() * f64::from(remaining)
                + correction.as_dvec3())
            .as_vec3();
            let length = delta.as_dvec3().length() as f32;
            if length.abs() >= VECTOR_EPSILON {
                delta = (delta.as_dvec3() / f64::from(length)).as_vec3();
                let Some(next) = self.sweep(state.position, delta, length)? else {
                    return Ok(false);
                };
                forward = next;
                delta *= forward.distance();
                horizontal_distance =
                    (f64::from(horizontal_distance) + delta.truncate().as_dvec2().length()) as f32;
                state.position += delta;
                up_distance += delta.z;
            }
        }
        let accepted =
            if forward.last_triangle().is_none() || horizontal_distance > self.interval.radius {
                let Some(downward) = self.sweep(state.position, Vec3::NEG_Z, up_distance)? else {
                    return Ok(false);
                };
                state.position.z -= downward.distance();
                if downward
                    .last_triangle()
                    .is_none_or(|index| self.geometry.triangles()[index].surface_normal().z > SLOPE)
                {
                    true
                } else {
                    self.fall_trial(state, original, heading, up_distance - downward.distance())?
                }
            } else {
                false
            };
        state.position = original;
        if accepted {
            state.step_anchor.get_or_insert(original.z);
        } else {
            state.step_anchor = None;
        }
        Ok(true)
    }

    fn fall_trial(
        &mut self,
        state: &mut MovementGroundSnapshot,
        original: Vec3,
        heading: Vec2,
        drop: f32,
    ) -> Result<bool, MovementGroundAdvanceError> {
        let duration_ms = MovementFallTrajectory::step_trial_millis(drop);
        if let Some(mut fall) = state.start_fall()? {
            // Native trial save/restore omits current XYZ and these two launch
            // fields, while restoring the clock, input bases, speed, and flags.
            state.launch_height = state.position.z;
            state.initial_downward_speed = 0.0;
            let displacement = state.horizontal_direction.extend(
                -MovementFallTrajectory::new(state.mode, 0.0)?.distance_at_millis(duration_ms)?,
            );
            let mut elapsed = 0_u32;
            while elapsed < duration_ms {
                let result = fall.advance_with_geometry(
                    MovementFallInterval {
                        duration_ms: duration_ms - elapsed,
                        displacement,
                        radius: self.interval.radius,
                        height: self.interval.height,
                        support_profile: self.interval.profile.support(),
                        policy: MovementFallAdvancePolicy::Trial,
                    },
                    self.geometry,
                )?;
                self.unavailable |= result.geometry_unavailable;
                self.skipped_time_ms = self.skipped_time_ms.wrapping_add(result.skipped_time_ms);
                elapsed = elapsed.wrapping_add(result.consumed_ms);
                match result.continuation {
                    MovementFallContinuation::Airborne(next) => {
                        let snapshot = next.snapshot();
                        state.position = snapshot.position;
                        state.launch_height = snapshot.launch_height;
                        state.initial_downward_speed = snapshot.initial_downward_speed;
                        fall = next;
                    }
                    MovementFallContinuation::Landed { position, .. } => {
                        state.position = position;
                        break;
                    }
                }
            }
        }
        let delta = state.position.truncate() - original.truncate();
        let length = delta.as_dvec2().length() as f32;
        Ok(length >= self.interval.radius
            && (delta.as_dvec2() / f64::from(length)).dot(heading.as_dvec2())
                > f64::from(TRIAL_DIRECTION_COSINE))
    }
}
