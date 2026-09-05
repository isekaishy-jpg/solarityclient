//! Build-12340 primary sequence timers used by explicit Model sequence requests.

use solarity_asset::{M2ModelAnimationMode, M2Sequence};

/// Whether sequence setup runs inside the scene's current update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2SequenceStartPhase {
    /// Script calls outside scene update start one millisecond after its tick.
    BeforeSceneUpdate,
    /// A sequence callback inside scene update uses the current tick directly.
    DuringSceneUpdate,
}

/// One native primary sequence timer, with wrapping millisecond arithmetic.
///
/// `0x00826B00` constructs the timer; `0x0082F0F0` samples its pose. Keeping
/// scene time separate from animation time lets a seek preserve live effects
/// and use the actual last-frame interval for event dispatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ModelSequenceTimer {
    start_ms: u32,
    end_ms: u32,
    duration_ms: u32,
    cycle_count: u32,
    initial_time_ms: u32,
    direction: i32,
    loops: bool,
}

impl M2ModelSequenceTimer {
    /// Constructs the exact unit-speed timer used by Model Lua methods.
    ///
    /// The caller supplies the next raw CRT roll for the authored cycle range.
    /// Float conversion before integer truncation and the optional one-tick
    /// adjustment intentionally follow the native instruction sequence.
    #[must_use]
    pub fn new(
        sequence: &M2Sequence,
        mode: M2ModelAnimationMode,
        scene_time_ms: u32,
        time_offset_ms: i32,
        cycle_roll: u16,
        phase: M2SequenceStartPhase,
    ) -> Self {
        Self::with_input_direction(
            sequence,
            mode,
            1,
            scene_time_ms,
            time_offset_ms,
            cycle_roll,
            phase,
        )
    }

    /// Reconstructs a timer from stock's automatic variation callback.
    ///
    /// `0x00831FC0` carries the previous speed and converts the callback's
    /// overdue scene time through its inverse before calling `0x00826B00`.
    /// It does not discard the portion of the current frame after the boundary.
    #[must_use]
    pub fn restart_variation(
        self,
        sequence: &M2Sequence,
        mode: M2ModelAnimationMode,
        scene_time_ms: u32,
        boundary_ms: u32,
        cycle_roll: u16,
    ) -> Self {
        let overdue = scene_time_ms.wrapping_sub(boundary_ms);
        let offset = native_float_word(overdue as f32 * self.direction as f32) as i32;
        Self::with_input_direction(
            sequence,
            mode,
            self.direction,
            scene_time_ms,
            offset,
            cycle_roll,
            M2SequenceStartPhase::DuringSceneUpdate,
        )
    }

    /// Implements timer setup for the unit-speed Lua path and its variation callback.
    fn with_input_direction(
        sequence: &M2Sequence,
        mode: M2ModelAnimationMode,
        input_direction: i32,
        scene_time_ms: u32,
        time_offset_ms: i32,
        cycle_roll: u16,
        phase: M2SequenceStartPhase,
    ) -> Self {
        let cycle_count = sequence.cycle_count(cycle_roll);
        let span = sequence.duration_ms().wrapping_mul(cycle_count);
        let direction = if matches!(
            mode,
            M2ModelAnimationMode::Reverse | M2ModelAnimationMode::HoldEnd
        ) {
            -input_direction
        } else {
            input_direction
        };
        let initial_time_ms = if direction < 0 { span } else { 0 };
        let direction = if matches!(
            mode,
            M2ModelAnimationMode::HoldStart | M2ModelAnimationMode::HoldEnd
        ) {
            0
        } else {
            direction
        };
        let offset = if direction == 0 {
            0
        } else {
            native_float_word(time_offset_ms as f32)
        };
        let start_ms = scene_time_ms
            .wrapping_sub(offset)
            .wrapping_add(u32::from(phase == M2SequenceStartPhase::BeforeSceneUpdate));
        let end_ms = start_ms.wrapping_add(if direction == 0 {
            0
        } else {
            native_float_word(span as f32)
        });
        Self {
            start_ms,
            end_ms,
            duration_ms: sequence.duration_ms(),
            cycle_count,
            initial_time_ms,
            direction,
            loops: sequence.flags() & 1 == 0,
        }
    }

    /// Returns the timer's scene-clock start, including any explicit seek.
    #[must_use]
    pub const fn start_time_ms(self) -> u32 {
        self.start_ms
    }

    /// Returns the native cycle-count deadline on the scene clock.
    #[must_use]
    pub const fn end_time_ms(self) -> u32 {
        self.end_ms
    }

    /// Returns the selected total play count.
    #[must_use]
    pub const fn cycle_count(self) -> u32 {
        self.cycle_count
    }

    /// Returns the first looping-sequence completion crossed by this scene interval.
    ///
    /// `0x00832260` places this callback at the last millisecond of a cycle,
    /// independently of the total play count stored by the primary timer.
    /// Held and flag-`0x1` sequences do not automatically select variations.
    #[must_use]
    pub fn next_loop_boundary_ms(self, previous_ms: u32, current_ms: u32) -> Option<u32> {
        if !self.loops
            || self.direction == 0
            || self.duration_ms == 0
            || previous_ms == current_ms
            || !tick_at_or_after(current_ms, previous_ms)
        {
            return None;
        }
        let elapsed = if tick_at_or_after(previous_ms, self.start_ms) {
            previous_ms.wrapping_sub(self.start_ms)
        } else {
            0
        };
        let boundary = self
            .start_ms
            .wrapping_add((elapsed / self.duration_ms + 1).wrapping_mul(self.duration_ms))
            .wrapping_sub(1);
        (boundary != previous_ms
            && tick_at_or_after(boundary, previous_ms)
            && tick_at_or_after(current_ms, boundary))
        .then_some(boundary)
    }

    /// Returns the authored sequence-local pose time at one scene tick.
    ///
    /// Looping sequences use unsigned remainder even before the start tick.
    /// Sequences with flag `0x1` clamp before start and hold their terminal pose.
    #[must_use]
    pub fn animation_time_ms(self, scene_time_ms: u32) -> u32 {
        if !self.loops && tick_at_or_after(scene_time_ms, self.end_ms) {
            return ((self.unwrapped_time(self.end_ms) as i32).max(0) as u32).min(self.duration_ms);
        }
        let scene_time_ms = if !self.loops && !tick_at_or_after(scene_time_ms, self.start_ms) {
            self.start_ms
        } else {
            scene_time_ms
        };
        if self.duration_ms == 0 {
            0
        } else {
            self.unwrapped_time(scene_time_ms) % self.duration_ms
        }
    }

    /// Reports each timestamp's occurrence in the native scene interval.
    /// `0x00830FB0` maps event keys back onto scene time; it does not replay a
    /// prefix from zero when sequence setup changes the timer's start.
    pub(super) fn visit_event_ticks(
        self,
        timestamps: &[u32],
        previous_ms: u32,
        current_ms: u32,
        mut visit: impl FnMut(u32),
    ) {
        // A terminal sequence's completion callback marks its timer finished.
        // Keys authored beyond that deadline cannot fire on a later frame.
        let current_ms = if !self.loops && tick_at_or_after(current_ms, self.end_ms) {
            self.end_ms
        } else {
            current_ms
        };
        if self.direction == 0
            || self.duration_ms == 0
            || previous_ms == current_ms
            || !tick_at_or_after(current_ms, previous_ms)
        {
            return;
        }
        let elapsed = if tick_at_or_after(previous_ms, self.start_ms) {
            previous_ms.wrapping_sub(self.start_ms)
        } else {
            0
        };
        let mut cycle_offset = if self.loops {
            (elapsed / self.duration_ms) * self.duration_ms
        } else {
            0
        };
        loop {
            for timestamp in timestamps {
                let delta = timestamp.wrapping_sub(self.initial_time_ms) as i32;
                let tick = self
                    .start_ms
                    .wrapping_add(cycle_offset)
                    .wrapping_add(native_float_word(delta as f32 * self.direction as f32));
                if tick != previous_ms
                    && tick_at_or_after(tick, previous_ms)
                    && tick_at_or_after(current_ms, tick)
                {
                    visit(tick);
                }
            }
            if !self.loops {
                break;
            }
            cycle_offset = cycle_offset.wrapping_add(self.duration_ms);
            if !tick_at_or_after(current_ms, self.start_ms.wrapping_add(cycle_offset)) {
                break;
            }
        }
    }

    /// Reproduces the signed scene delta, float product, and low-word addition.
    fn unwrapped_time(self, scene_time_ms: u32) -> u32 {
        native_float_word(
            scene_time_ms.wrapping_sub(self.start_ms) as i32 as f32 * self.direction as f32,
        )
        .wrapping_add(self.initial_time_ms)
    }
}

/// Native signed-difference comparisons remain valid across the u32 tick wrap.
fn tick_at_or_after(tick: u32, reference: u32) -> bool {
    tick.wrapping_sub(reference) as i32 >= 0
}

/// Every timer product is bounded by a u32 span; convert via i64 before EAX.
fn native_float_word(value: f32) -> u32 {
    value as i64 as u32
}
