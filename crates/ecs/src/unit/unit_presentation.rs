//! Display and posture state projected from stock unit fields.

use shipyard::Component;

/// Unit model selection and client presentation state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitPresentation {
    display_id: u32,
    native_display_id: u32,
    mount_display_id: u32,
    stand_state: u8,
}

impl UnitPresentation {
    /// Creates the complete typed presentation view.
    #[must_use]
    pub const fn new(
        display_id: u32,
        native_display_id: u32,
        mount_display_id: u32,
        stand_state: u8,
    ) -> Self {
        Self {
            display_id,
            native_display_id,
            mount_display_id,
            stand_state,
        }
    }

    /// Returns the active CreatureDisplayInfo.dbc identifier.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns the unit's unmorphed CreatureDisplayInfo.dbc identifier.
    #[must_use]
    pub const fn native_display_id(self) -> u32 {
        self.native_display_id
    }

    /// Returns the active mount CreatureDisplayInfo.dbc identifier.
    #[must_use]
    pub const fn mount_display_id(self) -> u32 {
        self.mount_display_id
    }

    /// Returns byte zero of `UNIT_FIELD_BYTES_1`.
    #[must_use]
    pub const fn stand_state(self) -> u8 {
        self.stand_state
    }
}
