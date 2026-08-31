//! Authoritative realm clock advanced from `SMSG_LOGIN_SETTIMESPEED`.

use std::time::{Duration, Instant};

use solarity_network::WorldTimeSpeed;

const DAY_HALF_MINUTES: f32 = 2_880.0;
const DAY_MILLISECONDS: f64 = 86_400_000.0;

/// A running realm clock anchored to the instant its server packet arrived.
///
/// There is deliberately no system-clock constructor. Stock world lighting is
/// unavailable until the realm supplies `SMSG_LOGIN_SETTIMESPEED`.
#[derive(Clone, Debug)]
pub struct RealmClock {
    source: WorldTimeSpeed,
    received_at: Instant,
}

impl RealmClock {
    /// Starts or replaces the running clock with an authoritative server value.
    #[must_use]
    pub fn new(source: WorldTimeSpeed) -> Self {
        Self {
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
