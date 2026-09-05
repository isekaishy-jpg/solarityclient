//! Property-specific stock animation sampling over typed timestamp keys.

use glam::{Quat, Vec3};
use solarity_asset::{M2AnimationSet, M2Interpolation, M2SplineKey, M2Track, M2TrackChannel};

use super::M2AnimationClock;
use super::quaternion::{blend_quaternion, interpolate_quaternion};

/// `0x0082B0A0` samples ordinary vectors linearly for every nonzero selector.
pub(crate) fn sample_vec3(
    animations: &M2AnimationSet,
    track: &M2Track<Vec3>,
    clock: M2AnimationClock,
    default: Vec3,
) -> Vec3 {
    sample_continuous(animations, track, clock, default)
}

/// `0x0082AF40`/`0x0082B340` sample ordinary scalars before sequence blending.
pub(crate) fn sample_scalar(
    animations: &M2AnimationSet,
    track: &M2Track<f32>,
    clock: M2AnimationClock,
    default: f32,
) -> f32 {
    sample_continuous(animations, track, clock, default)
}

/// Applies the previous-sequence blend after each ordinary track sample.
fn sample_continuous<T>(
    animations: &M2AnimationSet,
    track: &M2Track<T>,
    clock: M2AnimationClock,
    default: T,
) -> T
where
    T: Copy
        + std::ops::Add<Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Mul<f32, Output = T>,
{
    let primary = sample_linear_primary(animations, track, clock, default);
    let Some((secondary, weight)) = clock.secondary_for(track) else {
        return primary;
    };
    let previous = sample_linear_primary(animations, track, secondary, default);
    primary + (previous - primary) * weight
}

/// Camera vectors and roll use complete spline keys for every selector.
/// Roll remains an ordinary scalar in `0x0082B8A0`, with no angle wrapping.
pub(super) fn sample_spline<T>(
    animations: &M2AnimationSet,
    track: &M2Track<M2SplineKey<T>>,
    clock: M2AnimationClock,
    default: T,
) -> T
where
    T: Copy
        + std::ops::Add<Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Mul<f32, Output = T>,
{
    let primary = sample_spline_primary(animations, track, clock, default);
    let Some((secondary, weight)) = clock.secondary_for(track) else {
        return primary;
    };
    let previous = sample_spline_primary(animations, track, secondary, default);
    primary + (previous - primary) * weight
}

/// `0x00828680`/`0x0082AD50` blend complete compressed/float rotation samples.
pub(super) fn sample_quaternion(
    animations: &M2AnimationSet,
    track: &M2Track<Quat>,
    clock: M2AnimationClock,
) -> Quat {
    let primary = sample_quaternion_primary(animations, track, clock);
    let Some((secondary, weight)) = clock.secondary_for(track) else {
        return primary;
    };
    let previous = sample_quaternion_primary(animations, track, secondary);
    blend_quaternion(primary, previous, weight)
}

/// One selected channel and its resolved local/global clock time.
struct SampleLocation<'track, T> {
    channel: &'track M2TrackChannel<T>,
    time_ms: f32,
}

/// Selects a global channel or stock's sequence/channel-zero behavior.
fn locate<'track, T>(
    animations: &M2AnimationSet,
    track: &'track M2Track<T>,
    clock: M2AnimationClock,
) -> Option<SampleLocation<'track, T>> {
    if let Some(global) = track.global_sequence() {
        let duration = *animations
            .global_sequence_durations_ms()
            .get(usize::from(global))? as f32;
        let time_ms = if duration > 0.0 {
            clock.global_time_ms().rem_euclid(duration)
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
        .get(clock.sequence())
        .or_else(|| track.channels().first())?;
    Some(SampleLocation {
        channel,
        time_ms: clock.animation_time_ms(),
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

/// Ordinary values have one element per timestamp and no cubic controls.
fn sample_linear_primary<T>(
    animations: &M2AnimationSet,
    track: &M2Track<T>,
    clock: M2AnimationClock,
    default: T,
) -> T
where
    T: Copy
        + std::ops::Add<Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Mul<f32, Output = T>,
{
    let Some(location) = locate(animations, track, clock) else {
        return default;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return default;
    };
    let first = location.channel.values()[lower];
    if track.interpolation() == M2Interpolation::Step {
        return first;
    }
    let second = location.channel.values()[upper];
    first + (second - first) * amount
}

/// Interpolates complete typed camera keys in the native value/control order.
fn sample_spline_primary<T>(
    animations: &M2AnimationSet,
    track: &M2Track<M2SplineKey<T>>,
    clock: M2AnimationClock,
    default: T,
) -> T
where
    T: Copy
        + std::ops::Add<Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Mul<f32, Output = T>,
{
    let Some(location) = locate(animations, track, clock) else {
        return default;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return default;
    };
    let first = &location.channel.values()[lower];
    let second = &location.channel.values()[upper];
    match track.interpolation() {
        M2Interpolation::Step => *first.value(),
        M2Interpolation::Linear => *first.value() + (*second.value() - *first.value()) * amount,
        M2Interpolation::Bezier | M2Interpolation::Hermite => cubic(
            track.interpolation(),
            *first.value(),
            *first.outgoing(),
            *second.incoming(),
            *second.value(),
            amount,
        ),
    }
}

/// Step retains the decoded quaternion; every nonzero selector uses 0x00982630.
fn sample_quaternion_primary(
    animations: &M2AnimationSet,
    track: &M2Track<Quat>,
    clock: M2AnimationClock,
) -> Quat {
    let Some(location) = locate(animations, track, clock) else {
        return Quat::IDENTITY;
    };
    let Some((lower, upper, amount)) =
        interval(location.channel, track.interpolation(), location.time_ms)
    else {
        return Quat::IDENTITY;
    };
    let first = location.channel.values()[lower];
    if track.interpolation() == M2Interpolation::Step {
        return first;
    }
    interpolate_quaternion(first, location.channel.values()[upper], amount)
}

/// Selectors and enable tracks hold one ordinary integer value per timestamp.
pub(crate) fn sample_discrete<T>(
    animations: &M2AnimationSet,
    track: &M2Track<T>,
    clock: M2AnimationClock,
    default: T,
) -> T
where
    T: Copy,
{
    let Some(location) = locate(animations, track, clock) else {
        return default;
    };
    let Some((lower, _, _)) = interval(location.channel, M2Interpolation::Step, location.time_ms)
    else {
        return default;
    };
    location.channel.values()[lower]
}

/// `0x0082B460`/`0x0082B8A0` use outgoing-lower and incoming-upper controls.
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
