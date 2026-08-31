//! Display and posture state projected from stock unit fields.

use shipyard::Component;

/// Weapon presentation selected by byte zero of `UNIT_FIELD_BYTES_2`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum UnitSheathState {
    /// No weapon set is readied; equipped weapons use their sheath links.
    #[default]
    Unarmed = 0,
    /// Main-hand and off-hand weapons are readied.
    Melee = 1,
    /// The ranged weapon is readied.
    Ranged = 2,
}

impl TryFrom<u8> for UnitSheathState {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unarmed),
            1 => Ok(Self::Melee),
            2 => Ok(Self::Ranged),
            other => Err(other),
        }
    }
}

/// Unit model selection and client presentation state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitPresentation {
    display_id: u32,
    native_display_id: u32,
    mount_display_id: u32,
    stand_state: u8,
    sheath_state: UnitSheathState,
}

impl UnitPresentation {
    /// Creates the complete typed presentation view.
    #[must_use]
    pub const fn new(
        display_id: u32,
        native_display_id: u32,
        mount_display_id: u32,
        stand_state: u8,
        sheath_state: UnitSheathState,
    ) -> Self {
        Self {
            display_id,
            native_display_id,
            mount_display_id,
            stand_state,
            sheath_state,
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

    /// Returns byte zero of `UNIT_FIELD_BYTES_2` as a closed stock state.
    #[must_use]
    pub const fn sheath_state(self) -> UnitSheathState {
        self.sheath_state
    }
}
