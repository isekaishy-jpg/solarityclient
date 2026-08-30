//! Authoritative realm clock advanced from `SMSG_LOGIN_SETTIMESPEED`.

use std::time::{Duration, Instant};

use solarity_network::WorldTimeSpeed;

const DAY_HALF_MINUTES: f32 = 2_880.0;

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
}
