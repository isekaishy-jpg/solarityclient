//! Health and power values projected from the stock unit field range.

use shipyard::Component;

/// The client's retained health prediction, separate from replicated health.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component)]
pub struct UnitHealthPrediction {
    health: i32,
}

impl UnitHealthPrediction {
    /// Creates the native signed prediction image.
    #[must_use]
    pub const fn new(health: i32) -> Self {
        Self { health }
    }

    /// Returns the value selected by the predictedHealth CVar.
    #[must_use]
    pub const fn health(self) -> i32 {
        self.health
    }
}

/// Authoritative current and maximum resource values for one unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitVitals {
    health: u32,
    max_health: u32,
    powers: [u32; 7],
    max_powers: [u32; 7],
}

impl UnitVitals {
    /// Creates a complete typed view of the stock resource words.
    #[must_use]
    pub const fn new(health: u32, max_health: u32, powers: [u32; 7], max_powers: [u32; 7]) -> Self {
        Self {
            health,
            max_health,
            powers,
            max_powers,
        }
    }

    /// Returns current health.
    #[must_use]
    pub const fn health(self) -> u32 {
        self.health
    }

    /// Returns maximum health.
    #[must_use]
    pub const fn max_health(self) -> u32 {
        self.max_health
    }

    /// Returns all seven build-12340 power slots in wire order.
    #[must_use]
    pub const fn powers(self) -> [u32; 7] {
        self.powers
    }

    /// Returns all seven build-12340 maximum-power slots in wire order.
    #[must_use]
    pub const fn max_powers(self) -> [u32; 7] {
        self.max_powers
    }
}
