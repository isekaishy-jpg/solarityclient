//! Build-12340 primary sequence timers used by explicit Model sequence requests.

use solarity_asset::{M2ModelAnimationMode, M2Sequence};

#[cfg(test)]
#[path = "../../../tests/support/sequence_completion.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "../../../tests/support/sequence_speed.rs"]
mod speed_tests;

#[cfg(test)]
#[path = "../../../tests/support/ground_sequence.rs"]
mod ground_tests;

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
    speed: f32,
    inverse_speed: f32,
    loops: bool,
    secondary_clamps: bool,
}

impl M2ModelSequenceTimer {
    /// Returns `82DD80`'s primary-sequence weight toward full ground alignment.
    ///
    /// The caller supplies the resolved primary sequence's authored flags.
    /// This samples the unclamped, non-looped `8266B0` timer, including its
    /// explicit offset, rather than the bone pose's modulo animation clock.
    #[must_use]
    pub fn ground_alignment_weight(self, sequence_flags: u32, scene_time_ms: u32) -> f32 {
        let flags = sequence_flags & 0xe;
        if flags == 8 {
            return 1.0;
        }
        if flags != 2 && flags != 4 {
            return 0.0;
        }
        let weight = if self.speed == 0.0 {
            1.0
        } else if self.speed > 0.0 {
            // 8266B0 truncates a signed wrapping delta times speed to int64,
            // keeps its low word, and then adds the initial animation offset.
            let elapsed = self.unwrapped_time(scene_time_ms);
            let elapsed = native_float_word(f64::from(
                (f64::from(elapsed) / f64::from(self.speed)) as f32,
            )) as i32;
            let half_duration = self.end_ms.wrapping_sub(self.start_ms) >> 1;
            let weight = f64::from(elapsed) / f64::from(half_duration);
            // The original comparisons select one for unordered/zero-span
            // ratios. Preserve that rule instead of Rust's NaN clamp behavior.
            if weight < 0.0 {
                0.0
            } else if weight < 1.0 {
                weight
            } else {
                1.0
            }
        } else {
            0.0
        };
        // Flag 4 subtracts the retained quotient before its final float store.
        if flags == 4 {
            (1.0 - weight) as f32
        } else {
            weight as f32
        }
    }

    /// Constructs the exact unit-speed timer used by Model Lua methods.
    ///
    /// The caller supplies the next raw CRT roll for the authored cycle range.
    /// Integer truncation and the optional one-tick adjustment follow the
    /// native instruction sequence, retaining wider arithmetic until stores.
    #[must_use]
    pub fn new(
        sequence: &M2Sequence,
        mode: M2ModelAnimationMode,
        scene_time_ms: u32,
        time_offset_ms: i32,
        cycle_roll: u16,
        phase: M2SequenceStartPhase,
    ) -> Self {
        Self::with_speed(
            sequence,
            mode,
            1.0,
            scene_time_ms,
            time_offset_ms,
            cycle_roll,
            phase,
        )
    }

    /// Constructs a timer with the native primary sequence playback speed.
    ///
    /// Animation time advances by `speed` per scene millisecond. The native
    /// inverse-speed threshold also controls cycle and event deadlines.
    #[must_use]
    pub fn with_speed(
        sequence: &M2Sequence,
        mode: M2ModelAnimationMode,
        speed: f32,
        scene_time_ms: u32,
        time_offset_ms: i32,
        cycle_roll: u16,
        phase: M2SequenceStartPhase,
    ) -> Self {
        Self::with_timing(
            sequence.duration_ms(),
            sequence.cycle_count(cycle_roll),
            sequence.flags(),
            mode,
            speed,
            scene_time_ms,
            time_offset_ms,
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
        let offset = self.variation_time_offset(overdue);
        Self::with_speed(
            sequence,
            mode,
            self.speed,
            scene_time_ms,
            offset,
            cycle_roll,
            M2SequenceStartPhase::DuringSceneUpdate,
        )
    }

    fn variation_time_offset(self, overdue: u32) -> i32 {
        if (f64::from(self.speed) - 1.0).abs() < f64::from(f32::from_bits(0x3480_0000)) {
            overdue as i32
        } else {
            native_nearest_word((f64::from(overdue) * f64::from(self.inverse_speed)) as f32) as i32
        }
    }

    /// The timing stores shared by Model requests and automatic variations.
    #[allow(clippy::too_many_arguments)]
    fn with_timing(
        duration_ms: u32,
        cycle_count: u32,
        flags: u32,
        mode: M2ModelAnimationMode,
        input_speed: f32,
        scene_time_ms: u32,
        time_offset_ms: i32,
        phase: M2SequenceStartPhase,
    ) -> Self {
        let span = duration_ms.wrapping_mul(cycle_count);
        let speed = if matches!(
            mode,
            M2ModelAnimationMode::Reverse | M2ModelAnimationMode::HoldEnd
        ) {
            -input_speed
        } else {
            input_speed
        };
        let initial_time_ms = if speed < 0.0 { span } else { 0 };
        let speed = if matches!(
            mode,
            M2ModelAnimationMode::HoldStart | M2ModelAnimationMode::HoldEnd
        ) {
            0.0
        } else {
            speed
        };
        let inverse_speed = native_inverse_speed(speed);
        let offset = native_float_word(f64::from(time_offset_ms) * inverse_speed.abs());
        let start_ms = scene_time_ms
            .wrapping_sub(offset)
            .wrapping_add(u32::from(phase == M2SequenceStartPhase::BeforeSceneUpdate));
        let end_ms =
            start_ms.wrapping_add(native_float_word(f64::from(span) * inverse_speed.abs()));
        Self {
            start_ms,
            end_ms,
            duration_ms,
            cycle_count,
            initial_time_ms,
            speed,
            inverse_speed: inverse_speed as f32,
            loops: flags & 1 == 0,
            secondary_clamps: flags & 0x80 != 0,
        }
    }

    /// Changes speed without restarting the primary or its variation (`827000`).
    pub fn set_speed(&mut self, speed: f32, scene_time_ms: u32) {
        let elapsed = self.unwrapped_time(scene_time_ms) as i32;
        let inverse_speed = native_inverse_speed(speed);
        self.start_ms =
            scene_time_ms.wrapping_sub(native_float_word(f64::from(elapsed) * inverse_speed.abs()));
        self.end_ms = self.start_ms.wrapping_add(native_float_word(
            f64::from(self.duration_ms.wrapping_mul(self.cycle_count)) * inverse_speed.abs(),
        ));
        self.speed = speed;
        self.inverse_speed = inverse_speed as f32;
    }

    /// `826ED0` seeks the current sequence without selecting another variation.
    /// The timer keeps its repeat count, mode, and stored reciprocal; both
    /// products truncate before their low words become scene ticks.
    pub fn seek(&mut self, time_ms: i32, scene_time_ms: u32) {
        let inverse = f64::from(self.inverse_speed).abs();
        self.start_ms = scene_time_ms.wrapping_sub(native_float_word(f64::from(time_ms) * inverse));
        self.end_ms = self.start_ms.wrapping_add(native_float_word(
            f64::from(self.duration_ms.wrapping_mul(self.cycle_count)) * inverse,
        ));
    }

    /// Returns the retained primary playback speed.
    #[must_use]
    pub const fn speed(self) -> f32 {
        self.speed
    }

    /// Native completion scanning uses the stored reciprocal, with a unit-speed
    /// tolerance, rather than recomputing it from the speed.
    fn scene_cycle_duration_ms(self) -> u32 {
        let inverse = f64::from(self.inverse_speed).abs();
        if (inverse - 1.0).abs() < f64::from(f32::from_bits(0x3480_0000)) {
            self.duration_ms
        } else {
            // 832260 stores the product to float, then uses FISTP without
            // the timer constructor's truncation-mode control-word change.
            native_nearest_word((f64::from(self.duration_ms) * inverse) as f32)
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

    /// Moves the native primary/secondary deadlines while the model is paused.
    /// Pose sampling still uses the current scene tick (`0x0082F0F0`).
    pub fn shift_scene_time(&mut self, delta_ms: u32) {
        self.start_ms = self.start_ms.wrapping_add(delta_ms);
        self.end_ms = self.end_ms.wrapping_add(delta_ms);
    }

    /// Returns whether native completion finishes this timer permanently.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !self.loops
    }

    /// 826C40 marks zero-span and already-ended terminal requests finished
    /// immediately, before the model can enqueue their completion callback.
    #[must_use]
    pub fn finished_on_activation(self, scene_time_ms: u32) -> bool {
        self.start_ms == self.end_ms
            || (!self.loops && tick_at_or_after(scene_time_ms, self.end_ms))
    }

    /// Finds a user sequence callback, including a terminal sequence's deadline.
    ///
    /// The owner must suppress repeated terminal callbacks after dispatch.
    /// Native `0x00832260` also dispatches an already overdue terminal timer;
    /// unlike a loop boundary, its deadline need not follow the previous tick.
    #[must_use]
    pub fn next_completion_ms(self, previous_ms: u32, current_ms: u32) -> Option<u32> {
        if self.loops {
            self.next_loop_boundary_ms(previous_ms, current_ms)
        } else {
            (self.scene_cycle_duration_ms() != 0
                && previous_ms != current_ms
                && tick_at_or_after(current_ms, previous_ms)
                && tick_at_or_after(current_ms, self.end_ms))
            .then_some(self.end_ms)
        }
    }

    /// Returns the first looping-sequence completion crossed by this scene interval.
    ///
    /// `0x00832260` places this callback at the last millisecond of a cycle,
    /// independently of the total play count stored by the primary timer.
    /// Held and flag-`0x1` sequences do not automatically select variations.
    #[must_use]
    pub fn next_loop_boundary_ms(self, previous_ms: u32, current_ms: u32) -> Option<u32> {
        let duration_ms = self.scene_cycle_duration_ms();
        if !self.loops
            || duration_ms == 0
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
            .wrapping_add((elapsed / duration_ms + 1).wrapping_mul(duration_ms))
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
        self.sample_time_ms(scene_time_ms, !self.loops)
    }

    /// Samples a timer copied into the previous-pose slot during blending.
    ///
    /// `0x0082F0F0` tests sequence flag `0x80` here, whereas the primary slot
    /// tests flag `0x1`. Copying a finished primary timer does not by itself
    /// freeze the secondary pose.
    #[must_use]
    pub fn secondary_animation_time_ms(self, scene_time_ms: u32) -> u32 {
        self.sample_time_ms(scene_time_ms, self.secondary_clamps)
    }

    /// Shares native timer arithmetic while retaining the two completion flags.
    fn sample_time_ms(self, scene_time_ms: u32, clamp_to_deadline: bool) -> u32 {
        if clamp_to_deadline && tick_at_or_after(scene_time_ms, self.end_ms) {
            return ((self.unwrapped_time(self.end_ms) as i32).max(0) as u32).min(self.duration_ms);
        }
        let scene_time_ms = if clamp_to_deadline && !tick_at_or_after(scene_time_ms, self.start_ms)
        {
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
        let duration_ms = self.scene_cycle_duration_ms();
        if duration_ms == 0
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
            (elapsed / duration_ms) * duration_ms
        } else {
            0
        };
        loop {
            for timestamp in timestamps {
                let tick = self.event_tick_ms(*timestamp, cycle_offset);
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
            cycle_offset = cycle_offset.wrapping_add(duration_ms);
            if !tick_at_or_after(current_ms, self.start_ms.wrapping_add(cycle_offset)) {
                break;
            }
        }
    }

    fn event_tick_ms(self, timestamp: u32, cycle_offset: u32) -> u32 {
        let delta = timestamp.wrapping_sub(self.initial_time_ms) as i32;
        self.start_ms
            .wrapping_add(cycle_offset)
            .wrapping_add(native_float_word(
                f64::from(delta) * f64::from(self.inverse_speed),
            ))
    }

    /// Returns the fresh primary phase reported by `8266B0`, before wrapping
    /// or pose clamping. Unit sequence changes use it to preserve stride phase.
    #[must_use]
    pub fn unwrapped_time(self, scene_time_ms: u32) -> u32 {
        native_float_word(
            f64::from(scene_time_ms.wrapping_sub(self.start_ms) as i32) * f64::from(self.speed),
        )
        .wrapping_add(self.initial_time_ms)
    }
}

/// Native signed-difference comparisons remain valid across the u32 tick wrap.
fn tick_at_or_after(tick: u32, reference: u32) -> bool {
    tick.wrapping_sub(reference) as i32 >= 0
}

/// Native timer products truncate to a signed quadword before retaining EAX.
fn native_float_word(value: f64) -> u32 {
    value as i64 as u32
}

fn native_inverse_speed(speed: f32) -> f64 {
    if speed.abs() <= f32::from_bits(0x3727_c5ac) {
        0.0
    } else {
        1.0 / f64::from(speed)
    }
}

fn native_nearest_word(value: f32) -> u32 {
    let rounded = value.round_ties_even();
    if !(-2147483648.0..2147483648.0).contains(&rounded) {
        0x8000_0000
    } else {
        rounded as i32 as u32
    }
}
