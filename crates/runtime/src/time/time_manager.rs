//! Authoritative realm clock advanced from `SMSG_LOGIN_SETTIMESPEED`.

use std::time::{Duration, Instant};

use solarity_network::WorldTimeSpeed;

const DAY_HALF_MINUTES: f32 = 2_880.0;
const DAY_MILLISECONDS: f64 = 86_400_000.0;

/// One coherent cyclic time and calendar day sample for native sky effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RealmSkyTime {
    day_fraction: f32,
    calendar_days: i32,
}

impl RealmSkyTime {
    /// Applies 4F8410's authored Map.dbc minute override and resets lunar calendar phase.
    #[must_use]
    pub fn with_map_time_override(self, minutes: i32) -> Self {
        let day = (f64::from(minutes) * f64::from(0.000_694_444_46_f32)) as f32;
        Self {
            day_fraction: if (0.0..=1.0).contains(&day) { day } else { 0.5 },
            calendar_days: 0,
        }
    }
    /// Returns the continuous realm day in the native zero-to-one interval.
    #[must_use]
    pub const fn day_fraction(self) -> f32 {
        self.day_fraction
    }

    /// Returns 76C1F0's calendar midnight epoch-day index.
    #[must_use]
    pub const fn calendar_days(self) -> i32 {
        self.calendar_days
    }

    /// Returns the original sky owner's integer Light band time.
    #[must_use]
    pub fn half_minutes(self) -> u32 {
        (f64::from(self.day_fraction * DAY_HALF_MINUTES) - 0.5).round_ties_even() as u32
    }
}

/// A running realm clock anchored to the instant its server packet arrived.
///
/// There is deliberately no system-clock constructor. Stock world lighting is
/// unavailable until the realm supplies `SMSG_LOGIN_SETTIMESPEED`.
#[derive(Clone, Debug)]
pub struct RealmClock {
    source: WorldTimeSpeed,
    received_at: Instant,
    minute_clock: RealmMinuteClock,
}

/// The calendar owner advances only when its retained fraction is strictly
/// greater than one (76D900), independently of the continuous sky-day getter.
#[derive(Clone, Debug)]
struct RealmMinuteClock {
    minute: u16,
    fraction: f32,
    elapsed_ms: u32,
}

impl RealmMinuteClock {
    fn advance(&mut self, elapsed_ms: u32, rate: f32) {
        let seconds = elapsed_ms.wrapping_sub(self.elapsed_ms) as f32 * 0.001;
        self.elapsed_ms = elapsed_ms;
        self.fraction = (f64::from(rate) * f64::from(seconds) + f64::from(self.fraction)) as f32;
        // Preserve the native repeated f32 stores rather than quotient/remainder
        // division; exact integer boundaries remain in the preceding minute.
        while self.fraction > 1. {
            let next = self.fraction - 1.;
            if next == self.fraction {
                // The native loop stalls for an unrepresentable decrement.
                // Keep its clock state without hanging the presentation thread.
                break;
            }
            self.fraction = next;
            self.minute = (self.minute + 1) % 1440;
        }
    }
}

impl RealmClock {
    /// Advances the native whole-minute calendar owner once per client frame.
    pub(crate) fn advance(&mut self) {
        self.minute_clock.advance(
            self.received_at.elapsed().as_millis() as u32,
            self.source.game_time_speed(),
        );
    }

    /// Returns the realm calendar minute used for authored skybox animation.
    /// Map.dbc daylight overrides do not change this provider.
    #[must_use]
    pub const fn whole_minute(&self) -> i32 {
        self.minute_clock.minute as i32
    }
    /// Samples continuous sky time and the second moon's native calendar provider.
    /// Returns `None` if the platform cannot convert the realm calendar.
    #[must_use]
    pub fn sky_time(&self) -> Option<RealmSkyTime> {
        self.sky_time_after(self.received_at.elapsed())
    }

    /// Samples sky time after a deterministic duration, including calendar rollover.
    #[must_use]
    pub fn sky_time_after(&self, elapsed: Duration) -> Option<RealmSkyTime> {
        let base = f64::from(self.source.hour()) * 60.0 + f64::from(self.source.minute());
        // 76CFF0 uses the wrapping unsigned millisecond clock and retains x87
        // precision through both multiplications and the day wrap.
        let total = base
            + f64::from(elapsed.as_millis() as u32)
                * f64::from(0.001_f32)
                * f64::from(self.source.game_time_speed());
        let remainder = total.rem_euclid(1440.0);
        let minutes = if total > 0.0 && remainder == 0.0 {
            1440.0
        } else {
            remainder
        };
        let calendar_days = crate::platform::realm_calendar_days(
            self.source.year(),
            self.source.month_index(),
            self.source.month_day(),
            (total / 1440.0).floor() as u32,
        )?;
        Some(RealmSkyTime {
            day_fraction: (minutes * f64::from(0.000_694_444_46_f32)) as f32,
            calendar_days,
        })
    }
    /// Starts or replaces the running clock with an authoritative server value.
    #[must_use]
    pub fn new(source: WorldTimeSpeed) -> Self {
        Self {
            minute_clock: RealmMinuteClock {
                minute: u16::from(source.hour()) * 60 + u16::from(source.minute()),
                fraction: 0.,
                elapsed_ms: 0,
            },
            source,
            received_at: Instant::now(),
        }
    }

    /// Returns the exact packet currently anchoring the clock.
    #[must_use]
    pub const fn source(&self) -> WorldTimeSpeed {
        self.source
    }

    /// Returns the current cyclic half-minute used by stock DBC bands.
    #[must_use]
    pub fn half_minutes(&self) -> u32 {
        self.half_minutes_after(self.received_at.elapsed())
    }

    /// Returns the continuous cyclic time used by the sky's executable day tables.
    #[must_use]
    pub fn half_minutes_fraction(&self) -> f32 {
        self.half_minutes_fraction_after(self.received_at.elapsed())
    }

    /// Advances the clock by an explicit real duration.
    ///
    /// This pure-duration boundary also lets deterministic systems and external
    /// tests use the same stock conversion without sleeping or reading wall time.
    #[must_use]
    pub fn half_minutes_after(&self, elapsed: Duration) -> u32 {
        self.half_minutes_fraction_after(elapsed) as u32
    }

    /// Returns the fractional cyclic half-minute for continuous effects.
    #[must_use]
    pub fn half_minutes_fraction_after(&self, elapsed: Duration) -> f32 {
        let base = f32::from(self.source.hour()) * 120.0 + f32::from(self.source.minute()) * 2.0;
        let advanced = elapsed.as_secs_f32() * self.source.game_time_speed() * 2.0;
        (base + advanced).rem_euclid(DAY_HALF_MINUTES)
    }

    /// Returns current cyclic realm-day milliseconds for scheduled media.
    #[must_use]
    pub fn day_milliseconds(&self) -> u32 {
        self.day_milliseconds_after(self.received_at.elapsed())
    }

    /// Advances the realm clock and returns cyclic day milliseconds.
    ///
    /// Build 12340 evaluates advanced sound schedules from the same server-
    /// anchored day progression used by lighting. The integer conversion
    /// truncates the fractional millisecond after the 24-hour wrap.
    #[must_use]
    pub fn day_milliseconds_after(&self, elapsed: Duration) -> u32 {
        let base_minutes = f64::from(self.source.hour()) * 60.0 + f64::from(self.source.minute());
        let advanced_minutes = elapsed.as_secs_f64() * f64::from(self.source.game_time_speed());
        ((base_minutes + advanced_minutes) * 60_000.0).rem_euclid(DAY_MILLISECONDS) as u32
    }
}

#[cfg(test)]
mod minute_tests {
    use super::RealmMinuteClock;

    #[test]
    fn native_realm_whole_minute_frames() -> Result<(), Box<dyn std::error::Error>> {
        let mut clock = RealmMinuteClock {
            minute: 0,
            fraction: 0.,
            elapsed_ms: 0,
        };
        let mut rate = 0.;
        for (index, line) in include_str!("../../tests/fixtures/realm_minute_clock_native.txt")
            .lines()
            .enumerate()
        {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            match fields.first().copied() {
                Some("reset") => {
                    clock = RealmMinuteClock {
                        minute: fields[1].parse()?,
                        fraction: 0.,
                        elapsed_ms: fields[3].parse()?,
                    };
                    rate = f32::from_bits(u32::from_str_radix(fields[2], 16)?);
                }
                Some("tick") => {
                    clock.advance(fields[1].parse()?, rate);
                    assert_eq!(clock.minute, fields[2].parse::<u16>()?, "line {index}");
                    assert_eq!(
                        clock.fraction.to_bits(),
                        u32::from_str_radix(fields[3], 16)?,
                        "line {index}"
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }
}
