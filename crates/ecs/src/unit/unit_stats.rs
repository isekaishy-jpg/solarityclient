//! Primary attributes projected from the stock unit update-field range.

use shipyard::Component;

/// Number of primary attributes exposed by build 12340's `UnitStat` API.
pub const UNIT_PRIMARY_STAT_COUNT: usize = 5;

/// Authoritative effective values and their signed positive/negative modifiers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitStats {
    values: [i32; UNIT_PRIMARY_STAT_COUNT],
    positive_modifiers: [i32; UNIT_PRIMARY_STAT_COUNT],
    negative_modifiers: [i32; UNIT_PRIMARY_STAT_COUNT],
}

impl UnitStats {
    /// Creates a complete typed view in Strength-through-Spirit wire order.
    #[must_use]
    pub const fn new(
        values: [i32; UNIT_PRIMARY_STAT_COUNT],
        positive_modifiers: [i32; UNIT_PRIMARY_STAT_COUNT],
        negative_modifiers: [i32; UNIT_PRIMARY_STAT_COUNT],
    ) -> Self {
        Self {
            values,
            positive_modifiers,
            negative_modifiers,
        }
    }

    /// Returns the five current attributes in stock wire order.
    #[must_use]
    pub const fn values(self) -> [i32; UNIT_PRIMARY_STAT_COUNT] {
        self.values
    }

    /// Returns the five positive modifiers in stock wire order.
    #[must_use]
    pub const fn positive_modifiers(self) -> [i32; UNIT_PRIMARY_STAT_COUNT] {
        self.positive_modifiers
    }

    /// Returns the five negative modifiers in stock wire order.
    #[must_use]
    pub const fn negative_modifiers(self) -> [i32; UNIT_PRIMARY_STAT_COUNT] {
        self.negative_modifiers
    }
}
