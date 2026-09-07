//! Native acceleration profiles, preserving x87 intermediates and float spills.

use super::route::TransportRouteError;

/// Native ramp placement within a continuous section.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum LegProfile {
    /// Decelerate from the section entrance to its first stop.
    Decelerating,
    /// Accelerate from the last stop toward the section exit.
    Accelerating,
    /// Acceleration followed by deceleration between two stops.
    BetweenStops,
    /// A section without any stops.
    Constant,
}

/// `007F8660`/`007F7A60` round a float millisecond duration with FISTP.
pub(super) fn duration_ms(
    distance: f64,
    speed: f64,
    acceleration: f64,
    profile: LegProfile,
) -> Result<u32, TransportRouteError> {
    let inverse_acceleration = 1.0 / acceleration;
    let ramp_time = speed * inverse_acceleration;
    let ramp_distance = speed * 0.5 * ramp_time;
    let seconds = match profile {
        LegProfile::Constant => distance / speed,
        LegProfile::Decelerating | LegProfile::Accelerating if distance <= ramp_distance => {
            (2.0 * inverse_acceleration * distance).sqrt()
        }
        LegProfile::Decelerating | LegProfile::Accelerating => {
            (distance - ramp_distance) / speed + ramp_time
        }
        LegProfile::BetweenStops if distance * 0.5 <= ramp_distance => {
            2.0 * (inverse_acceleration * distance).sqrt()
        }
        LegProfile::BetweenStops => (distance - ramp_distance * 2.0) / speed + ramp_time * 2.0,
    };
    let milliseconds = ((seconds * 1000.0) as f32).round_ties_even();
    // Native FISTP assumes each leg fits a signed 32-bit millisecond count.
    // Reject unsupported input instead of Rust's saturating float-to-int cast.
    if !(0.0..2_147_483_648.0).contains(&milliseconds) {
        return Err(TransportRouteError::UnrepresentableDuration);
    }
    Ok(milliseconds as u32)
}

/// Distance, model sequence, and instantaneous speed from `007F7660`/`007F76F0`.
/// Those callees receive float seconds; the final boundary leg stays in x87.
pub(super) fn sample_leg(
    elapsed: f64,
    duration: f64,
    speed: f64,
    acceleration: f64,
    profile: LegProfile,
) -> (f64, u32, f32) {
    let full_ramp = speed / acceleration;
    if profile == LegProfile::Constant {
        return (elapsed * speed, 0xa3, speed as f32);
    }
    if profile == LegProfile::Accelerating {
        if elapsed <= full_ramp.min(duration) {
            return (
                acceleration * elapsed * 0.5 * elapsed,
                0xa2,
                (acceleration * elapsed) as f32,
            );
        }
        return (
            (elapsed - full_ramp) * speed + speed * 0.5 * full_ramp,
            0xa3,
            speed as f32,
        );
    }
    let elapsed = f64::from(elapsed as f32);
    let duration = f64::from(duration as f32);
    if profile == LegProfile::Decelerating {
        let ramp = full_ramp.min(duration);
        let cruise = duration - ramp;
        if elapsed <= cruise {
            return (elapsed * speed, 0xa3, speed as f32);
        }
        let peak = acceleration * ramp;
        let deceleration_time = elapsed - cruise;
        return (
            cruise * speed
                + (peak * 2.0 - acceleration * deceleration_time) * 0.5 * deceleration_time,
            0xa4,
            (peak - acceleration * deceleration_time) as f32,
        );
    }
    let ramp = full_ramp.min(duration * 0.5);
    if duration - ramp < elapsed {
        let peak = acceleration * ramp;
        let deceleration_time = elapsed - (duration - ramp);
        return (
            (duration - 2.0 * ramp) * speed
                + (peak * 2.0 - acceleration * deceleration_time) * 0.5 * deceleration_time
                + peak * 0.5 * ramp,
            0xa4,
            (peak - acceleration * deceleration_time) as f32,
        );
    }
    if ramp < elapsed {
        return (
            (elapsed - full_ramp) * speed + speed * 0.5 * full_ramp,
            0xa3,
            speed as f32,
        );
    }
    (
        elapsed * (acceleration * elapsed) * 0.5,
        0xa2,
        (acceleration * elapsed) as f32,
    )
}
