//! Stock step, linear, Bezier, and Hermite track evaluation.

use glam::{Quat, Vec3};
use solarity_asset::{M2AnimationSet, M2Interpolation, M2Track, M2TrackChannel};

/// One selected channel and its resolved local/global clock time.
struct SampleLocation<'track, T> {
    channel: &'track M2TrackChannel<T>,
    time_ms: f32,
}

/// Selects a global channel or stock's sequence/channel-zero behavior.
fn locate<'track, T>(
    animations: &M2AnimationSet,
    track: &'track M2Track<T>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
) -> Option<SampleLocation<'track, T>> {
    if let Some(global) = track.global_sequence() {
        let duration = *animations
            .global_sequence_durations_ms()
            .get(usize::from(global))? as f32;
        let time_ms = if duration > 0.0 {
            global_time_ms.rem_euclid(duration)
        } else {
            0.0
        };
        return track
            .channels()
            .first()
            .map(|channel| SampleLocation { channel, time_ms });
    }
    let channel = track
        .channels()
        .get(sequence)
        .or_else(|| track.channels().first())?;
    Some(SampleLocation {
        channel,
        time_ms: animation_time_ms,
    })
}

/// Returns the lower key and normalized interval fraction.
fn interval<T>(
    channel: &M2TrackChannel<T>,
    interpolation: M2Interpolation,
    time_ms: f32,
) -> Option<(usize, usize, f32)> {
    let timestamps = channel.timestamps_ms();
    if timestamps.is_empty() {
        return None;
    }
    let upper = timestamps.partition_point(|timestamp| *timestamp as f32 <= time_ms);
    let lower = upper.saturating_sub(1).min(timestamps.len() - 1);
    let upper = (lower + 1).min(timestamps.len() - 1);
    if interpolation == M2Interpolation::Step || lower == upper {
        return Some((lower, upper, 0.0));
    }
    let start = timestamps[lower] as f32;
    let end = timestamps[upper] as f32;
    let fraction = if end > start {
        ((time_ms - start) / (end - start)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some((lower, upper, fraction))
}

/// Returns the value-array stride expressed in typed values.
const fn values_per_key(interpolation: M2Interpolation) -> usize {
    match interpolation {
        M2Interpolation::Step | M2Interpolation::Linear => 1,
        M2Interpolation::Bezier | M2Interpolation::Hermite => 3,
    }
}

/// Evaluates one vector-valued bone track.
pub(crate) fn sample_vec3(
    animations: &M2AnimationSet,
    track: &M2Track<Vec3>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
    default: Vec3,
) -> Vec3 {
    let Some(location) = locate(
        animations,
        track,
        sequence,
        animation_time_ms,
        global_time_ms,
    ) else {
        return default;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return default;
    };
    let stride = values_per_key(track.interpolation());
    let Some(&first) = location.channel.values().get(lower * stride) else {
        return default;
    };
    let Some(&second) = location.channel.values().get(upper * stride) else {
        return default;
    };
    interpolate_vec3(
        location.channel,
        track.interpolation(),
        lower,
        upper,
        amount,
        first,
        second,
    )
}

/// Evaluates one scalar material track with the shared cubic basis.
pub(crate) fn sample_scalar(
    animations: &M2AnimationSet,
    track: &M2Track<f32>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
    default: f32,
) -> f32 {
    let Some(location) = locate(
        animations,
        track,
        sequence,
        animation_time_ms,
        global_time_ms,
    ) else {
        return default;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return default;
    };
    let stride = values_per_key(track.interpolation());
    let Some(&first) = location.channel.values().get(lower * stride) else {
        return default;
    };
    let Some(&second) = location.channel.values().get(upper * stride) else {
        return default;
    };
    match track.interpolation() {
        M2Interpolation::Step => first,
        M2Interpolation::Linear => first + (second - first) * amount,
        M2Interpolation::Bezier | M2Interpolation::Hermite => {
            let values = location.channel.values();
            cubic(
                track.interpolation(),
                first,
                values[lower * 3 + 2],
                values[upper * 3 + 1],
                second,
                amount,
            )
        }
    }
}

/// Evaluates the wrapped linear angle used by build-12340 M2 cameras.
///
/// Camera roll is not an ordinary scalar interpolation. The Northrend login
/// camera authors equivalent endpoints at one full turn and zero; stock takes
/// the signed remainder of their delta before interpolation, keeping that
/// camera static instead of revolving the entire Glue scene once per cycle.
pub(crate) fn sample_angle_radians(
    animations: &M2AnimationSet,
    track: &M2Track<f32>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
    default: f32,
) -> f32 {
    if track.interpolation() != M2Interpolation::Linear {
        return sample_scalar(
            animations,
            track,
            sequence,
            animation_time_ms,
            global_time_ms,
            default,
        );
    }
    let Some(location) = locate(
        animations,
        track,
        sequence,
        animation_time_ms,
        global_time_ms,
    ) else {
        return default;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return default;
    };
    let Some(&first) = location.channel.values().get(lower) else {
        return default;
    };
    let Some(&second) = location.channel.values().get(upper) else {
        return default;
    };
    if !first.is_finite() || !second.is_finite() {
        return default;
    }
    let full_turn = core::f32::consts::TAU;
    let authored_delta = second - first;
    let wrapped_delta = authored_delta - full_turn * (authored_delta / full_turn).round_ties_even();
    first + wrapped_delta * amount
}

/// Applies the selected vector interpolation using WotLK's value/tangent order.
fn interpolate_vec3(
    channel: &M2TrackChannel<Vec3>,
    interpolation: M2Interpolation,
    lower: usize,
    upper: usize,
    amount: f32,
    first: Vec3,
    second: Vec3,
) -> Vec3 {
    match interpolation {
        M2Interpolation::Step => first,
        M2Interpolation::Linear => first.lerp(second, amount),
        M2Interpolation::Bezier | M2Interpolation::Hermite => {
            let values = channel.values();
            let outgoing = values[lower * 3 + 2];
            let incoming = values[upper * 3 + 1];
            cubic(interpolation, first, outgoing, incoming, second, amount)
        }
    }
}

/// Evaluates one compressed-quaternion bone track.
pub(super) fn sample_quaternion(
    animations: &M2AnimationSet,
    track: &M2Track<Quat>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
) -> Quat {
    let Some(location) = locate(
        animations,
        track,
        sequence,
        animation_time_ms,
        global_time_ms,
    ) else {
        return Quat::IDENTITY;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return Quat::IDENTITY;
    };
    let stride = values_per_key(track.interpolation());
    let Some(&first) = location.channel.values().get(lower * stride) else {
        return Quat::IDENTITY;
    };
    if lower == upper || track.interpolation() == M2Interpolation::Step {
        return first;
    }
    let Some(mut second) = location.channel.values().get(upper * stride).copied() else {
        return Quat::IDENTITY;
    };
    if first.dot(second) < 0.0 {
        second = -second;
    }
    let sampled = match track.interpolation() {
        M2Interpolation::Step => first,
        M2Interpolation::Linear => first * (1.0 - amount) + second * amount,
        M2Interpolation::Bezier | M2Interpolation::Hermite => {
            let values = location.channel.values();
            let outgoing = values[lower * 3 + 2];
            let incoming = values[upper * 3 + 1];
            cubic(
                track.interpolation(),
                first,
                outgoing,
                incoming,
                second,
                amount,
            )
        }
    };
    sampled.normalize()
}

/// Evaluates a selector or enable track without inventing fractional states.
///
/// Build 12340 stores ribbon texture slots and visibility in ordinary M2
/// tracks, but consumes their values as discrete integers. The selected key is
/// therefore held even when malformed content labels the track as a spline.
pub(crate) fn sample_discrete<T>(
    animations: &M2AnimationSet,
    track: &M2Track<T>,
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
    default: T,
) -> T
where
    T: Copy,
{
    let Some(location) = locate(
        animations,
        track,
        sequence,
        animation_time_ms,
        global_time_ms,
    ) else {
        return default;
    };
    let Some((lower, _upper, _amount)) =
        interval(location.channel, M2Interpolation::Step, location.time_ms)
    else {
        return default;
    };
    let stride = values_per_key(track.interpolation());
    location
        .channel
        .values()
        .get(lower * stride)
        .copied()
        .unwrap_or(default)
}

/// Shares the recovered cubic basis between vectors and quaternions.
fn cubic<T>(
    interpolation: M2Interpolation,
    first: T,
    outgoing: T,
    incoming: T,
    second: T,
    amount: f32,
) -> T
where
    T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>,
{
    let squared = amount * amount;
    let cubed = squared * amount;
    if interpolation == M2Interpolation::Bezier {
        let inverse = 1.0 - amount;
        return first * (inverse * inverse * inverse)
            + outgoing * (3.0 * inverse * inverse * amount)
            + incoming * (3.0 * inverse * squared)
            + second * cubed;
    }
    first * (2.0 * cubed - 3.0 * squared + 1.0)
        + outgoing * (cubed - 2.0 * squared + amount)
        + second * (-2.0 * cubed + 3.0 * squared)
        + incoming * (cubed - squared)
}
