//! Native 710570..7108C6 state-driven transport clock.

use std::num::NonZeroU32;

/// Retained type-11 behavior clock, independent of model playback and frame time.
///
/// The optional period is the last position key's timestamp; no position rows
/// means no period. `split_ms` arguments are the current GAMEOBJECT_LEVEL, which
/// separates the state-zero interval from all other states. State bytes retain
/// the native signed conversion. A zero split selects continuous looping.
#[derive(Clone, Copy, Debug)]
pub struct TransportAnimationClock {
    period_ms: Option<NonZeroU32>,
    cached_state: i8,
    anchor_ms: u32,
    retained_ms: u32,
}

impl TransportAnimationClock {
    /// Captures 711F20's creation state and 7105D0's initial progress anchor.
    #[must_use]
    pub fn new(
        period_ms: Option<NonZeroU32>,
        state: i8,
        split_ms: u32,
        raw_time_ms: u32,
        progress: u16,
    ) -> Self {
        let mut clock = Self {
            period_ms,
            cached_state: state,
            anchor_ms: 0,
            retained_ms: 0,
        };
        if split_ms != 0 && period_ms.is_some() {
            // 7105D0 keeps both products extended, spills once to f32, then
            // FISTP uses round-to-nearest/even. FFFF is an ordinary fraction.
            let elapsed = (f64::from(progress)
                * f64::from(f32::from_bits(0x3780_0080))
                * f64::from(clock.duration_ms(split_ms, state))) as f32;
            clock.anchor_ms = raw_time_ms.wrapping_sub(fistp(f64::from(elapsed)));
        }
        clock
    }

    /// 710640's phase for the current replicated state, without endpoint mutation.
    #[must_use]
    pub fn phase_ms(&self, split_ms: u32, state: i8, raw_time_ms: u32) -> u32 {
        let Some(period) = self.period_ms else {
            return raw_time_ms;
        };
        if split_ms == 0 {
            return raw_time_ms % period.get();
        }
        let elapsed = raw_time_ms.wrapping_sub(self.anchor_ms);
        let phase = if state == self.cached_state {
            elapsed.wrapping_add(self.retained_ms)
        } else {
            self.retained_ms.saturating_sub(elapsed)
        };
        if self.cached_state == 0 {
            phase.min(split_ms)
        } else {
            let phase = phase.wrapping_add(split_ms);
            if phase >= period.get() { 0 } else { phase }
        }
    }

    /// 7139E0 publishes a passenger phase using the retained state before sampling.
    #[must_use]
    pub fn passenger_phase_ms(&self, split_ms: u32, raw_time_ms: u32) -> u32 {
        self.phase_ms(split_ms, self.cached_state, raw_time_ms)
    }

    /// 7106D0/70DA40 settle a reversed interval when its endpoint is sampled.
    pub fn sample_phase_ms(&mut self, split_ms: u32, state: i8, raw_time_ms: u32) -> u32 {
        let phase = self.phase_ms(split_ms, state, raw_time_ms);
        let endpoint = if self.cached_state == state && self.cached_state == 0 {
            split_ms
        } else {
            0
        };
        if self.retained_ms == 0 || phase != endpoint {
            return phase;
        }
        self.cached_state = state;
        self.retained_ms = 0;
        if state == 0 {
            self.anchor_ms = raw_time_ms.wrapping_sub(split_ms);
            split_ms
        } else {
            self.anchor_ms = raw_time_ms
                .wrapping_add(split_ms)
                .wrapping_sub(self.period_ms.map_or(0, NonZeroU32::get));
            0
        }
    }

    /// Applies 710820's old/new state callback at the receipt's raw transport time.
    /// A reversal before 95% retains the previous interval; later changes start
    /// the new state's interval. Repeated states can still reverse retained motion.
    pub fn notify_state(&mut self, split_ms: u32, old: i8, new: i8, raw_time_ms: u32) {
        if split_ms == 0 || self.period_ms.is_none() || self.cached_state == new {
            return;
        }
        let progress = self.progress(split_ms, old, raw_time_ms);
        self.anchor_ms = raw_time_ms;
        if progress < f64::from(f32::from_bits(0x3f73_3333)) {
            let elapsed = (progress * f64::from(self.duration_ms(split_ms, old))) as f32;
            self.retained_ms = fistp(f64::from(elapsed) - 0.5);
        } else {
            self.cached_state = new;
            self.retained_ms = 0;
        }
    }

    fn duration_ms(&self, split_ms: u32, state: i8) -> u32 {
        let Some(period) = self.period_ms else {
            return 0;
        };
        if split_ms == 0 {
            period.get()
        } else if state == 0 {
            split_ms
        } else {
            period.get().wrapping_sub(split_ms)
        }
    }

    /// 710780 measures progress in the cached interval, reflecting it for old state.
    fn progress(&self, split_ms: u32, state: i8, raw_time_ms: u32) -> f64 {
        let mut phase = self.phase_ms(split_ms, state, raw_time_ms);
        let duration = self.duration_ms(split_ms, self.cached_state);
        if self.cached_state != 0 {
            if phase == 0 {
                phase = self.period_ms.map_or(0, NonZeroU32::get);
            }
            phase = phase.wrapping_sub(split_ms);
        }
        if state != self.cached_state {
            phase = duration.wrapping_sub(phase);
        }
        f64::from(phase) / f64::from(duration)
    }
}

/// x87's signed integer-indefinite result is observable through wrapping clocks.
fn fistp(value: f64) -> u32 {
    let rounded = value.round_ties_even();
    if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        i32::MIN as u32
    } else {
        rounded as i32 as u32
    }
}
