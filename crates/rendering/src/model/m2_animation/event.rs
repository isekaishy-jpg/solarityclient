//! Interval-crossing selection for timestamp-only M2 event tracks.

use solarity_asset::M2AnimationSet;

use super::sequence_timer::M2ModelSequenceTimer;

/// One monotonic event interval for a selected M2 animation sequence.
///
/// Bone and material sampling consume wrapped clocks. Events instead require
/// unwrapped elapsed time so a long frame and a loop boundary still cross each
/// authored timestamp exactly once for that update. The optional global
/// interval is likewise unwrapped and shared by every global-sequence event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2EventTimeWindow {
    sequence: usize,
    previous_animation_time_ms: f32,
    animation_time_ms: f32,
    include_start: bool,
    loops: bool,
    global_time_ms: Option<(f32, f32)>,
    scene_timer: Option<(M2ModelSequenceTimer, u32, u32)>,
    queued_event: Option<usize>,
}

impl M2EventTimeWindow {
    /// Creates a sequence-local event interval without a global-clock source.
    #[must_use]
    pub const fn new(
        sequence: usize,
        previous_animation_time_ms: f32,
        animation_time_ms: f32,
        include_start: bool,
        loops: bool,
    ) -> Self {
        Self {
            sequence,
            previous_animation_time_ms,
            animation_time_ms,
            include_start,
            loops,
            global_time_ms: None,
            scene_timer: None,
            queued_event: None,
        }
    }

    /// One occurrence already selected by the shared native bone callback
    /// queue. Re-scanning its interval would duplicate coincident authored keys.
    #[must_use]
    pub const fn queued_event(sequence: usize, event_index: usize) -> Self {
        let mut window = Self::new(sequence, 0.0, 0.0, false, false);
        window.queued_event = Some(event_index);
        window
    }

    /// Supplies the process-global interval used by global-sequence events.
    #[must_use]
    pub const fn with_global_time(
        mut self,
        previous_global_time_ms: f32,
        global_time_ms: f32,
    ) -> Self {
        self.global_time_ms = Some((previous_global_time_ms, global_time_ms));
        self
    }

    /// Uses the native primary timer's scene interval after an explicit Model request.
    ///
    /// Unlike elapsed-time windows, this preserves seeks, reverse playback,
    /// integer ticks, and repeated occurrences in native timestamp order.
    #[must_use]
    pub const fn with_scene_timer(
        mut self,
        timer: M2ModelSequenceTimer,
        previous_scene_time_ms: u32,
        scene_time_ms: u32,
    ) -> Self {
        self.scene_timer = Some((timer, previous_scene_time_ms, scene_time_ms));
        self
    }
}

/// Returns event declaration indices whose timelines cross one interval.
///
/// The legacy elapsed-time window returns each declaration at most once, even
/// when several timestamps or loop occurrences fall inside a long frame. This
/// remains a compatibility gap for callers without a native scene timer. A
/// scene-timer window returns every occurrence in native timestamp order,
/// including repeated indices. An invalid or unavailable interval produces no
/// callbacks and never selects another sequence.
#[must_use]
pub fn triggered_m2_event_indices(
    animations: &M2AnimationSet,
    window: M2EventTimeWindow,
) -> Vec<usize> {
    if let Some(index) = window.queued_event {
        return if index < animations.events().len() {
            vec![index]
        } else {
            Vec::new()
        };
    }
    if let Some((timer, previous, current)) = window.scene_timer {
        return triggered_scene_timer_events(animations, window.sequence, timer, previous, current);
    }
    if !valid_interval(window.previous_animation_time_ms, window.animation_time_ms) {
        return Vec::new();
    }
    let Some(sequence) = animations.resolve_sequence_alias(window.sequence) else {
        return Vec::new();
    };
    let sequence_duration = animations.sequences()[sequence].duration_ms();
    let global_interval = window
        .global_time_ms
        .filter(|(previous, current)| valid_interval(*previous, *current));
    let mut triggered = Vec::new();
    for (index, event) in animations.events().iter().enumerate() {
        let (timestamps, duration, previous, current) =
            if let Some(global_sequence) = event.timeline().global_sequence() {
                let Some(duration) = animations
                    .global_sequence_durations_ms()
                    .get(usize::from(global_sequence))
                    .copied()
                else {
                    continue;
                };
                let Some(timestamps) = event.timeline().channels().first() else {
                    continue;
                };
                let Some((previous, current)) = global_interval else {
                    continue;
                };
                (timestamps, duration, previous, current)
            } else {
                let Some(timestamps) = event.timeline().channels().get(sequence) else {
                    continue;
                };
                (
                    timestamps,
                    if window.loops { sequence_duration } else { 0 },
                    window.previous_animation_time_ms,
                    window.animation_time_ms,
                )
            };
        if timestamps.iter().copied().any(|timestamp| {
            crosses_timestamp(timestamp, duration, previous, current, window.include_start)
        }) {
            triggered.push(index);
        }
    }
    triggered
}

/// 0x00832260/0x00830FB0 dispatch every crossed occurrence in scene-time order.
fn triggered_scene_timer_events(
    animations: &M2AnimationSet,
    sequence: usize,
    timer: M2ModelSequenceTimer,
    previous: u32,
    current: u32,
) -> Vec<usize> {
    let Some(sequence) = animations.resolve_sequence_alias(sequence) else {
        return Vec::new();
    };
    let mut occurrences = Vec::new();
    for (index, event) in animations.events().iter().enumerate() {
        // Native global-sequence event declarations select channel zero here;
        // their keys still pass through the primary timer's scene mapping.
        let channel = if event.timeline().global_sequence().is_some() {
            0
        } else {
            sequence
        };
        let Some(timestamps) = event.timeline().channels().get(channel) else {
            continue;
        };
        timer.visit_event_ticks(timestamps, previous, current, |tick| {
            occurrences.push((tick.wrapping_sub(previous), index));
        });
    }
    occurrences.sort_by_key(|(time, index)| (*time, *index));
    occurrences
        .into_iter()
        .map(|(_time, index)| index)
        .collect()
}

fn valid_interval(previous: f32, current: f32) -> bool {
    previous.is_finite() && current.is_finite() && current >= 0.0 && current >= previous
}

fn crosses_timestamp(
    timestamp: u32,
    duration: u32,
    previous: f32,
    current: f32,
    include_start: bool,
) -> bool {
    let timestamp = f64::from(timestamp);
    let previous = f64::from(previous).max(0.0);
    let current = f64::from(current);
    if duration == 0 {
        return if include_start {
            timestamp >= previous && timestamp <= current
        } else {
            timestamp > previous && timestamp <= current
        };
    }

    let duration = f64::from(duration);
    let cycle = ((previous - timestamp) / duration).ceil().max(0.0);
    let mut occurrence = timestamp + cycle * duration;
    if (!include_start && occurrence <= previous) || (include_start && occurrence < previous) {
        occurrence += duration;
    }
    occurrence <= current
}
