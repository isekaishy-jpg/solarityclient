//! Previous-sequence ownership and the native smoothstep blend envelope.

use super::{M2AnimationClock, M2ModelSequenceTimer};

/// A primary timer copied into the native previous-pose slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ModelSequenceBlend {
    sequence: usize,
    timer: M2ModelSequenceTimer,
    end_ms: u32,
    inverse_duration: f32,
    amplitude: f32,
}

impl M2ModelSequenceBlend {
    /// Returns the authored sequence occupying the secondary slot.
    #[must_use]
    pub const fn sequence(self) -> usize {
        self.sequence
    }

    /// Returns the wrapping scene deadline for the secondary contribution.
    #[must_use]
    pub const fn end_time_ms(self) -> u32 {
        self.end_ms
    }

    /// Copies the old timer when `0x00826C40` starts a new automatic blend.
    ///
    /// The envelope starts at the current scene tick, even when the callback
    /// carries overdue time. Its duration belongs to the incoming sequence.
    #[must_use]
    pub fn new(
        sequence: usize,
        timer: M2ModelSequenceTimer,
        scene_time_ms: u32,
        duration_ms: u32,
    ) -> Self {
        Self {
            sequence,
            timer,
            end_ms: scene_time_ms.wrapping_add(duration_ms),
            inverse_duration: if duration_ms == 0 {
                1.0
            } else {
                1.0 / duration_ms as f32
            },
            amplitude: 1.0,
        }
    }

    /// `826DD0` installs a wound in the secondary slot at 75% strength.
    /// Its envelope lasts one authored clip, independently of cycle count.
    #[must_use]
    pub fn wound(
        sequence: usize,
        timer: M2ModelSequenceTimer,
        scene_time_ms: u32,
        duration_ms: u32,
    ) -> Self {
        Self {
            amplitude: 0.75,
            ..Self::new(sequence, timer, scene_time_ms, duration_ms)
        }
    }

    /// Returns the previous pose's remaining contribution at one scene tick.
    #[must_use]
    pub fn weight(self, scene_time_ms: u32) -> f32 {
        let remaining = self.end_ms.wrapping_sub(scene_time_ms) as i32;
        if remaining <= 0 {
            return 0.0;
        }
        let fraction = (remaining as f32 * self.inverse_duration).clamp(0.0, 1.0);
        (3.0 - (fraction + fraction)) * fraction * fraction * self.amplitude
    }

    /// Pauses the secondary pose while retaining the original blend envelope.
    /// Native `0x0082F0F0` shifts the copied timer but not the blend deadline.
    pub fn shift_pose_time(&mut self, delta_ms: u32) {
        self.timer.shift_scene_time(delta_ms);
    }

    /// Adds the old sequence's independently advancing pose to the new clock.
    #[must_use]
    pub fn apply_to_clock(self, clock: M2AnimationClock, scene_time_ms: u32) -> M2AnimationClock {
        let weight = self.weight(scene_time_ms);
        let animation_time_ms = self.timer.secondary_animation_time_ms(scene_time_ms) as f32;
        // 0x0082F779 suppresses blending when both sampled sequence/time pairs
        // coincide, in addition to the ordinary envelope deadline.
        if weight == 0.0
            || (self.sequence == clock.sequence() && animation_time_ms == clock.animation_time_ms())
        {
            return clock;
        }
        clock.with_secondary_sequence(self.sequence, animation_time_ms, weight)
    }
}
