//! Engine-clock texture sequence selection from native 8A1D60.

use std::num::NonZeroU32;

/// A resident liquid texture sequence with one shared engine-clock period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiquidTextureTimeline {
    frame_count: NonZeroU32,
    period_ms: NonZeroU32,
}

impl LiquidTextureTimeline {
    /// Retains the authored period, including stock's zero-to-one normalization.
    #[must_use]
    pub const fn new(frame_count: NonZeroU32, period_ms: u32) -> Self {
        Self {
            frame_count,
            period_ms: match NonZeroU32::new(period_ms) {
                Some(period) => period,
                None => NonZeroU32::MIN,
            },
        }
    }

    /// Selects the original texture-array ordinal at an unsigned engine clock.
    ///
    /// `8A1D60` spills the scaled phase to float, subtracts one half, then uses
    /// nearest-even FISTP. Exact frame boundaries therefore differ from floor.
    /// A one-frame sequence bypasses the clock entirely.
    #[must_use]
    pub fn frame_index(self, time_ms: u32) -> u32 {
        if self.frame_count.get() == 1 {
            return 0;
        }
        let period_ms = self.period_ms.get();
        let phase = (f64::from(time_ms % period_ms) / f64::from(period_ms)
            * f64::from(self.frame_count.get())) as f32;
        (f64::from(phase) - 0.5).round_ties_even() as u32
    }
}
