//! Collision-driven fall loop recovered from `0x007612B0`.

use crate::movement::clock::{millis_from_seconds, seconds_from_millis};
use glam::Vec3;

use super::state::{
    MovementFallAdvance, MovementFallAdvanceError, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallPhase, MovementFallState,
};
use crate::collision::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementFallContactKind,
};
use crate::movement::{
    MovementFallContactError, MovementFallContactQuery, MovementFallMode, MovementFallTrajectory,
    MovementGeometry, geometry::FixedMovementGeometry,
};

// Exact native fields at 0x009F1224, 0x00A25098, 0x00A37F84, 0x00A1EA9C,
// and 0x009E1134. These differ from rounded decimal approximations.
const DEGENERATE_TOLERANCE: f32 = f32::from_bits(0x3580_0000);
const MINIMUM_PROGRESS_SECONDS: f32 = f32::from_bits(0x3a03_126f);
const SLIDE_GROWTH_PER_VERTICAL: f32 = f32::from_bits(0x3f97_e4ad);
const FAR_FALL_HEIGHT: f32 = f32::from_bits(0x3de3_8e39);

impl MovementFallState {
    /// Applies the native repeated-contact fall loop to an admitted interval.
    ///
    /// Candidates must cover every repeated query in one coordinate space;
    /// collection failure/transport conversion belongs to the world provider.
    /// The result owns the updated state and explicit anchor/phase actions.
    /// No caller state, packet queue, world cache, or input state is mutated.
    ///
    /// # Errors
    /// Returns [`MovementFallAdvanceError`] for invalid geometry/displacement or
    /// an unrepresentable contact result. The original copy remains reusable.
    pub fn advance(
        self,
        interval: MovementFallInterval,
        triangles: &[MovementCollisionTriangle],
    ) -> Result<MovementFallAdvance, MovementFallAdvanceError> {
        self.advance_with_geometry(interval, &mut FixedMovementGeometry(triangles))
    }

    /// Advances through a provider that refreshes geometry before every sweep.
    ///
    /// The provider must have complete initial interval coverage. Collection
    /// failure retains prior motion, reports full consumed time, and excludes
    /// the unavailable remainder from the fall clock. The outer owner applies
    /// `skipped_time_ms` to its analytic clock and skipped-time/heartbeat owner.
    /// Copied contact identity survives a later failed or reordered candidate query.
    ///
    /// # Errors
    /// Returns an error for invalid state, geometry, or unrepresentable arithmetic.
    /// Unavailable collection is reported with the native partial continuation.
    pub fn advance_with_geometry<G: MovementGeometry + ?Sized>(
        self,
        interval: MovementFallInterval,
        geometry: &mut G,
    ) -> Result<MovementFallAdvance<G::TriangleIdentity>, MovementFallAdvanceError> {
        let mut state = self.snapshot;
        let original = state;
        let mut volume =
            MovementCollisionVolume::new(state.position, interval.radius, interval.height)
                .map_err(MovementFallContactError::from)?;
        let mut delta = interval.displacement;
        let mut distance = delta.as_dvec3().length() as f32;
        if !delta.is_finite() || !distance.is_finite() {
            return Err(MovementFallContactError::Sweep(
                crate::collision::MovementSweepError::InvalidDisplacement,
            )
            .into());
        }
        let total_seconds = f64::from(seconds_from_millis(interval.duration_ms));
        let fall_seconds = seconds_from_millis(state.fall_time_ms);
        let mut elapsed = 0.0_f32;
        let mut consumed_seconds = total_seconds;
        let mut horizontal_distance = delta.truncate().as_dvec2().length() as f32;
        let original_horizontal_distance = horizontal_distance;
        let original_vertical_distance = delta.z.abs();
        let mut small_steps = 0;
        let mut iterations = 0_u32;
        let mut horizontal_stopped = false;
        let mut landed = false;
        let mut ceiling = false;
        let mut reset_motion_anchor = false;
        let mut contact_triangle = None;
        let mut geometry_unavailable = false;
        let mut skipped_time_ms = 0;
        while distance.abs() >= DEGENERATE_TOLERANCE {
            let direction = (delta.as_dvec3() / f64::from(distance)).as_vec3();
            // The contact helper's sweep divides the stored delta by the stored
            // length. Its cache must cover that same extrusion, not a re-created
            // product from the response loop's wider direction calculation.
            if !geometry.prepare_sweep(&volume, delta / distance, distance) {
                geometry_unavailable = true;
                skipped_time_ms = millis_from_seconds(total_seconds - f64::from(elapsed));
                state.fall_time_ms = state.fall_time_ms.wrapping_sub(skipped_time_ms);
                break;
            }
            let trajectory = MovementFallTrajectory::new(state.mode, state.initial_downward_speed)
                .map_err(MovementFallContactError::from)?;
            let (contact, contact_seconds) = MovementFallContactQuery {
                trajectory,
                launch_height: state.launch_height,
                elapsed_seconds: fall_seconds + elapsed,
                interval_seconds: (total_seconds - f64::from(elapsed)) as f32,
                horizontal_speed: state.horizontal_speed,
                support_profile: interval.support_profile,
            }
            .resolve_extended(&volume, delta, geometry.triangles())?;
            contact_triangle = if matches!(
                contact.kind,
                MovementFallContactKind::Clear | MovementFallContactKind::Ceiling
            ) {
                None
            } else {
                contact
                    .last_triangle
                    .and_then(|index| geometry.triangle_identity(index))
            };
            let progress = if contact.kind == MovementFallContactKind::Clear {
                total_seconds - f64::from(elapsed)
            } else {
                contact_seconds
            };
            state.position += contact.displacement;
            let accumulated = f64::from(elapsed) + progress;
            elapsed = accumulated as f32;
            if contact.kind == MovementFallContactKind::Ceiling {
                state.phase = MovementFallPhase::FallingFar;
                state.fall_time_ms = 0;
                state.initial_downward_speed = 0.0;
                state.launch_height = state.position.z;
                ceiling = true;
                consumed_seconds = accumulated;
                break;
            }
            if contact.kind == MovementFallContactKind::Land {
                landed = true;
                break;
            }
            if total_seconds <= accumulated + f64::from(MINIMUM_PROGRESS_SECONDS) {
                break;
            }
            reset_motion_anchor = true;
            let remaining = f64::from(distance) - f64::from(contact.distance);
            if progress <= f64::from(MINIMUM_PROGRESS_SECONDS) {
                small_steps += 1;
            } else {
                small_steps = 1;
            }
            if small_steps >= 6 {
                // Stock first retries vertically, then ends the fall if that
                // attempt also cannot progress. This is not a frame-count cap.
                if horizontal_stopped || horizontal_distance.abs() < DEGENERATE_TOLERANCE {
                    landed = true;
                    break;
                }
                small_steps = 0;
                horizontal_stopped = true;
                state.horizontal_speed = 0.0;
                delta = Vec3::new(
                    0.0,
                    0.0,
                    (f64::from(direction.z) * f64::from(remaining as f32)) as f32,
                );
                distance = delta.z.abs();
            } else {
                delta = (direction.as_dvec3() * remaining
                    + contact.horizontal_correction.as_dvec2().extend(0.0))
                .as_vec3();
                if iterations != 0 {
                    // After the first retry, cap horizontal expansion by both
                    // the original horizontal request and native slope growth.
                    let traveled = state.position.truncate() - original.position.truncate();
                    let total_horizontal = traveled.as_dvec2() + delta.truncate().as_dvec2();
                    let limit =
                        f64::from(original_horizontal_distance) + f64::from(DEGENERATE_TOLERANCE);
                    let vertical_limit = f64::from(original_vertical_distance)
                        * f64::from(SLIDE_GROWTH_PER_VERTICAL);
                    if total_horizontal.length_squared() > limit * limit
                        && total_horizontal.length_squared() > vertical_limit * vertical_limit
                    {
                        landed = true;
                        break;
                    }
                }
                horizontal_distance = delta.truncate().as_dvec2().length() as f32;
                let mut horizontal_direction = delta.truncate();
                state.horizontal_speed = if horizontal_distance.abs() >= DEGENERATE_TOLERANCE {
                    horizontal_direction = (horizontal_direction.as_dvec2()
                        / f64::from(horizontal_distance))
                    .as_vec2();
                    (horizontal_direction
                        .as_dvec2()
                        .dot(state.horizontal_direction.as_dvec2())
                        * f64::from(state.horizontal_speed))
                    .max(0.0) as f32
                } else {
                    0.0
                };
                state.horizontal_direction = horizontal_direction;
                state.direction = horizontal_direction.extend(0.0);
                distance = delta.as_dvec3().length() as f32;
            }
            iterations = iterations.wrapping_add(1);
            volume = MovementCollisionVolume::new(state.position, interval.radius, interval.height)
                .map_err(MovementFallContactError::from)?;
        }
        if landed {
            // End-fall reloads the accumulated float clock after rebasing its
            // ground motion; a ceiling instead retains the wider sum above.
            consumed_seconds = f64::from(elapsed);
            reset_motion_anchor = true;
        }
        let consumed_ms = millis_from_seconds(consumed_seconds);
        if !ceiling {
            state.fall_time_ms = state.fall_time_ms.wrapping_add(consumed_ms);
        }
        if interval.policy != MovementFallAdvancePolicy::Trial && !landed {
            // Live phase promotion precedes restoration of an actively steered
            // jump's launch basis. Trials skip both native post-update actions.
            if state.phase == MovementFallPhase::Falling
                && state.mode == MovementFallMode::Normal
                && ((state.initial_downward_speed == 0.0 && state.fall_time_ms > 499)
                    || (state.initial_downward_speed != 0.0
                        && f64::from(state.position.z)
                            <= f64::from(state.launch_height) - f64::from(FAR_FALL_HEIGHT)))
            {
                state.phase = MovementFallPhase::FallingFar;
            }
            if interval.policy == MovementFallAdvancePolicy::LiveTranslating
                && state.initial_downward_speed != 0.0
            {
                state.horizontal_speed = original.horizontal_speed;
                state.horizontal_direction = original.horizontal_direction;
                state.direction = original.direction;
            }
        }
        let continuation = if landed {
            MovementFallContinuation::Landed {
                position: state.position,
                fall_time_ms: state.fall_time_ms,
            }
        } else {
            MovementFallContinuation::Airborne(Self::new(state)?)
        };
        Ok(MovementFallAdvance {
            consumed_ms,
            continuation,
            reset_motion_anchor,
            notify_ceiling_reset: ceiling && interval.policy != MovementFallAdvancePolicy::Trial,
            geometry_unavailable,
            skipped_time_ms,
            contact_triangle: if interval.policy == MovementFallAdvancePolicy::Trial {
                None
            } else {
                contact_triangle
            },
        })
    }
}
