//! Stock unit identity projected from the authoritative update-field table.

use shipyard::Component;

/// Stable identity values used to classify and present a unit.
///
/// Race, class, gender, and power type are the four bytes of
/// `UNIT_FIELD_BYTES_0`; the remaining values are their dedicated stock words.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitIdentity {
    race_id: u8,
    class_id: u8,
    gender_id: u8,
    power_type_id: u8,
    level: u32,
    faction_template_id: u32,
}

impl UnitIdentity {
    /// Creates the typed identity view without validating DBC-backed IDs.
    #[must_use]
    pub const fn new(
        race_id: u8,
        class_id: u8,
        gender_id: u8,
        power_type_id: u8,
        level: u32,
        faction_template_id: u32,
    ) -> Self {
        Self {
            race_id,
            class_id,
            gender_id,
            power_type_id,
            level,
            faction_template_id,
        }
    }

    /// Returns the ChrRaces.dbc identifier.
    #[must_use]
    pub const fn race_id(self) -> u8 {
        self.race_id
    }

    /// Returns the ChrClasses.dbc identifier.
    #[must_use]
    pub const fn class_id(self) -> u8 {
        self.class_id
    }

    /// Returns the stock gender identifier.
    #[must_use]
    pub const fn gender_id(self) -> u8 {
        self.gender_id
    }

    /// Returns the power type encoded by the server.
    #[must_use]
    pub const fn power_type_id(self) -> u8 {
        self.power_type_id
    }

    /// Returns the server-visible unit level.
    #[must_use]
    pub const fn level(self) -> u32 {
        self.level
    }

    /// Returns the FactionTemplate.dbc identifier.
    #[must_use]
    pub const fn faction_template_id(self) -> u32 {
        self.faction_template_id
    }
}
