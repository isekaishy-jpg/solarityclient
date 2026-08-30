//! Server-controlled unit flag words used by gameplay and presentation systems.

use shipyard::Component;

/// Raw stock unit flag sets retained without policy reinterpretation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitFlags {
    primary: u32,
    secondary: u32,
    dynamic: u32,
}

impl UnitFlags {
    /// Creates the typed view of all projected unit flag words.
    #[must_use]
    pub const fn new(primary: u32, secondary: u32, dynamic: u32) -> Self {
        Self {
            primary,
            secondary,
            dynamic,
        }
    }

    /// Returns `UNIT_FIELD_FLAGS` exactly as sent by the server.
    #[must_use]
    pub const fn primary(self) -> u32 {
        self.primary
    }

    /// Returns `UNIT_FIELD_FLAGS_2` exactly as sent by the server.
    #[must_use]
    pub const fn secondary(self) -> u32 {
        self.secondary
    }

    /// Returns `UNIT_DYNAMIC_FLAGS` exactly as sent by the server.
    #[must_use]
    pub const fn dynamic(self) -> u32 {
        self.dynamic
    }
}
