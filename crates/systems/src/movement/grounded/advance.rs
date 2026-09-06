//! Native repeated-contact ground loop at `0x007620F0`.

use super::correction::wall_correction;
use super::{
    DISTANCE_EPSILON, GroundQuery, MINIMUM_PROGRESS, MovementGroundAdvance,
    MovementGroundAdvanceError, MovementGroundContinuation, MovementGroundInterval,
    MovementGroundState, SLOPE, VECTOR_EPSILON,
};
use crate::collision::MovementCollisionTriangle;
use crate::movement::{MovementGeometry, geometry::FixedMovementGeometry};
use glam::Vec3;

const DOWN_PROBE_PER_DISTANCE: f32 = f32::from_bits(0x3fec_b91b);
const APPROACH_LIMIT: f32 = f32::from_bits(0xb727_c5ac);

impl MovementGroundState {
    /// Applies stock ground contact, slope following, step trials, and fall
    /// admission using a complete ordered candidate set for every probe.
    ///
    /// The outer owner resolves timestamped input, candidate coverage, body
    /// dimensions, mode gates, and resource identities before this boundary.
    ///
    /// # Errors
    /// Returns an error for invalid interval/geometry or unrepresentable trial
    /// state. No caller state or external notification is mutated on failure.
    pub fn advance(
        self,
        interval: MovementGroundInterval,
        triangles: &[MovementCollisionTriangle],
    ) -> Result<MovementGroundAdvance, MovementGroundAdvanceError> {
        self.advance_with_geometry(interval, &mut FixedMovementGeometry(triangles))
    }

    /// Advances against mutable geometry, refreshing coverage before each probe.
    ///
    /// The provider must start with a complete interval query. Collection
    /// failure returns the native partial continuation and full consumed time,
    /// without final reanchoring or contact notification on that exit path.
    /// Contact identities are copied before private probes can replace arrays.
    ///
    /// # Errors
    /// Returns an error for invalid state, geometry, or unrepresentable arithmetic.
    /// Provider unavailability is reported in the successful partial result.
    pub fn advance_with_geometry<G: MovementGeometry + ?Sized>(
        self,
        interval: MovementGroundInterval,
        geometry: &mut G,
    ) -> Result<MovementGroundAdvance<G::TriangleIdentity>, MovementGroundAdvanceError> {
        let mut query = GroundQuery {
            interval,
            geometry,
            unavailable: false,
            skipped_time_ms: 0,
        };
        let mut state = self.snapshot;
        query.volume(state.position)?;
        if !interval.distance.is_finite()
            || interval.distance < 0.0
            || !interval.direction.is_finite()
            || !interval.profile.step_height().is_finite()
            || interval.profile.step_height() < 0.0
        {
            return Err(MovementGroundAdvanceError::InvalidState);
        }
        let mut result = MovementGroundAdvance {
            consumed_ms: interval.duration_ms,
            continuation: MovementGroundContinuation::Grounded(self),
            reset_motion_anchor: false,
            contact_triangle: None,
            geometry_unavailable: false,
            skipped_time_ms: 0,
        };
        if interval.distance.abs() < DISTANCE_EPSILON {
            return Ok(result);
        }
        if query.geometry.triangles().is_empty() {
            if let Some(fall) = state.start_fall()? {
                result.continuation = MovementGroundContinuation::Falling(fall);
                result.reset_motion_anchor = true;
                result.consumed_ms = 0;
            }
            return Ok(result);
        }
        let total = interval.duration_ms as f32;
        let mut remaining_ms = total;
        let mut elapsed = 0.0_f32;
        let mut distance = interval.distance;
        let mut horizontal_remaining = distance;
        let mut heading = interval.direction;
        let mut direction = heading.extend(0.0);
        let mut climb_budget = distance / interval.radius;
        let mut tiny_progress = 0_u32;
        let mut force_fall = false;
        let mut reanchor = false;
        let mut contact;
        let mut contact_identity;
        loop {
            let Some(sweep) = query.sweep(state.position, direction, distance)? else {
                return query.interrupted(state);
            };
            contact = sweep.last_triangle();
            contact_identity = contact.and_then(|index| query.geometry.triangle_identity(index));
            let mut allowed = f64::from(sweep.distance());
            // Native FSTs expose float delta fields for horizontal accounting
            // while retaining wider products through position and progress.
            let mut delta_extended = direction.as_dvec3() * allowed;
            if delta_extended.z <= f64::from(climb_budget) {
                climb_budget = (f64::from(climb_budget) - delta_extended.z) as f32;
            } else {
                let scale = f64::from(climb_budget) / delta_extended.z;
                reanchor = true;
                delta_extended *= scale;
                allowed *= scale;
                climb_budget = 0.0;
            }
            let delta = delta_extended.as_vec3();
            state.position = (state.position.as_dvec3() + delta_extended).as_vec3();
            let Some(index) = contact else {
                elapsed += remaining_ms;
                let remaining = total - elapsed;
                if remaining < 1.0 {
                    let Some(down) = query.sweep(
                        state.position,
                        Vec3::NEG_Z,
                        (f64::from(interval.distance) * f64::from(DOWN_PROBE_PER_DISTANCE)) as f32,
                    )?
                    else {
                        return query.interrupted(state);
                    };
                    state.position.z -= down.distance();
                    contact = down.last_triangle();
                    contact_identity =
                        contact.and_then(|index| query.geometry.triangle_identity(index));
                    if let Some(index) = contact {
                        let normal = query.geometry.triangles()[index].surface_normal();
                        if normal.z > SLOPE {
                            state.step_anchor = None;
                        } else if state.step_anchor.is_none()
                            || normal.truncate().as_dvec2().dot(heading.as_dvec2())
                                > f64::from(APPROACH_LIMIT)
                        {
                            force_fall = true;
                        }
                    } else if state.step_anchor.is_none() {
                        force_fall = true;
                    }
                    break;
                }
                heading = interval.direction;
                direction = heading.extend(0.0);
                distance =
                    (f64::from(remaining) / f64::from(total) * f64::from(interval.distance)) as f32;
                remaining_ms = remaining;
                continue;
            };
            let progress = (allowed / f64::from(distance) * f64::from(remaining_ms)) as f32;
            distance = (f64::from(distance) - allowed) as f32;
            horizontal_remaining =
                (f64::from(horizontal_remaining) - delta.truncate().as_dvec2().length()) as f32;
            if progress < 1.0 {
                tiny_progress = tiny_progress.wrapping_add(1);
                if tiny_progress > 5 {
                    reanchor = true;
                    break;
                }
            } else {
                tiny_progress = 1;
            }
            elapsed += progress;
            remaining_ms -= progress;
            if total - elapsed < 1.0 {
                reanchor |= (total - elapsed).abs() >= DISTANCE_EPSILON;
                break;
            }
            let normal = query.geometry.triangles()[index].surface_normal();
            let (follow, start_fall) = query.classify(&mut state, index)?;
            force_fall = start_fall;
            let foot = sweep.combined_foot_normal();
            let correction = if follow {
                let original_distance = distance;
                let correction =
                    query.follow_surface(&state, direction, &mut distance, normal, foot);
                remaining_ms = (f64::from(distance) / f64::from(original_distance)
                    * f64::from(remaining_ms)) as f32;
                correction
            } else {
                if force_fall {
                    break;
                }
                reanchor = true;
                let was_step = state.step_anchor.is_some();
                if !query.step(&mut state, heading, normal)? {
                    return query.interrupted(state);
                }
                if state.step_anchor.is_some() {
                    let original_distance = distance;
                    let correction =
                        query.follow_surface(&state, direction, &mut distance, normal, foot);
                    remaining_ms = (f64::from(distance) / f64::from(original_distance)
                        * f64::from(remaining_ms)) as f32;
                    correction
                } else {
                    if was_step {
                        force_fall = true;
                        break;
                    }
                    wall_correction(direction, distance, horizontal_remaining, normal)
                }
            };
            let next_extended = direction.as_dvec3() * f64::from(distance) + correction.as_dvec3();
            let next = next_extended.as_vec3();
            distance = next_extended.length() as f32;
            if distance < MINIMUM_PROGRESS {
                reanchor = true;
                break;
            }
            // Length uses retained corrected products; normalization reloads
            // stored XYZ and retains the reciprocal until each XYZ store.
            direction = (next.as_dvec3() / f64::from(distance)).as_vec3();
            heading = next.truncate();
            let horizontal_extended = heading.as_dvec2().length();
            let horizontal = horizontal_extended as f32;
            if horizontal.abs() >= VECTOR_EPSILON {
                heading = (heading.as_dvec2() / horizontal_extended).as_vec2();
            }
            reanchor |= f64::from(horizontal_remaining) - f64::from(horizontal)
                > f64::from(DISTANCE_EPSILON);
            horizontal_remaining = horizontal;
            if let Some(anchor) = state.step_anchor
                && next.z > DISTANCE_EPSILON
            {
                let target = next.z + state.position.z;
                let top = f64::from(interval.profile.step_height()) + f64::from(anchor);
                if top < f64::from(target) {
                    let remaining = top - f64::from(state.position.z);
                    distance = (remaining / f64::from(next.z) * f64::from(distance)) as f32;
                    reanchor |= (remaining as f32 - next.z).abs() >= DISTANCE_EPSILON;
                    if distance < MINIMUM_PROGRESS {
                        force_fall = true;
                        break;
                    }
                }
            }
        }
        result.geometry_unavailable = query.unavailable;
        result.skipped_time_ms = query.skipped_time_ms;
        // Native rechecks the saved index against the possibly replaced array,
        // while notification uses the owner copied before the private probes.
        let contact_valid = contact.is_some_and(|index| index < query.geometry.triangles().len());
        if force_fall || (!contact_valid && state.step_anchor.is_none()) {
            if let Some(fall) = state.start_fall()? {
                result.continuation = MovementGroundContinuation::Falling(fall);
                result.reset_motion_anchor = true;
                return Ok(result);
            }
        } else if contact_valid {
            result.reset_motion_anchor = reanchor;
            result.contact_triangle = contact_identity;
        }
        result.continuation = MovementGroundContinuation::Grounded(Self::new(state)?);
        Ok(result)
    }
}
