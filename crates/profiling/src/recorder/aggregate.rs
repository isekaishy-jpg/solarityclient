//! Fixed histogram plus exact sums/maxima; no individual-event heap storage.

/// Histogram uses four subdivisions per power of two, starting at eight µs.
/// The final bin also contains longer samples; the exact maximum is retained.
#[derive(Clone)]
pub(crate) struct Aggregate {
    pub(crate) count: u64,
    pub(crate) sum: u64,
    pub(crate) maximum: u64,
    pub(crate) histogram: [u64; 64],
}

impl Default for Aggregate {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0,
            maximum: 0,
            histogram: [0; 64],
        }
    }
}

impl Aggregate {
    /// Combines intervals without retaining the individual observed events.
    pub(crate) fn merge(&mut self, other: &Self) {
        self.count = self.count.saturating_add(other.count);
        self.sum = self.sum.saturating_add(other.sum);
        self.maximum = self.maximum.max(other.maximum);
        for (sum, value) in self.histogram.iter_mut().zip(other.histogram) {
            *sum = sum.saturating_add(value);
        }
    }

    /// Records one duration or workload value with saturating totals.
    pub(crate) fn record(&mut self, value: u64) {
        self.count = self.count.saturating_add(1);
        self.sum = self.sum.saturating_add(value);
        self.maximum = self.maximum.max(value);
        let scaled = value.div_ceil(8_000).max(1);
        let exponent = scaled.ilog2();
        let base = 1_u64 << exponent;
        let subdivision = (scaled - base).saturating_mul(4) / base;
        let bucket = (u64::from(exponent) * 4 + subdivision).min(63) as usize;
        self.histogram[bucket] = self.histogram[bucket].saturating_add(1);
    }

    /// Returns a histogram upper bound, with the overflow bin using the exact max.
    pub(crate) fn percentile(&self, numerator: u64) -> u64 {
        let target = self.count.saturating_mul(numerator).div_ceil(100);
        let mut count: u64 = 0;
        for (index, samples) in self.histogram.iter().enumerate() {
            count = count.saturating_add(*samples);
            if count >= target {
                if index == 63 {
                    return self.maximum;
                }
                let base = 1_u64 << (index / 4);
                return ((base * (5 + index as u64 % 4)).div_ceil(4) * 8_000).min(self.maximum);
            }
        }
        self.maximum
    }
}
