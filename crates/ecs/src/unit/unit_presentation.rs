//! Display and posture state projected from stock unit fields.

use shipyard::Component;

/// Animation family selected by byte three of `UNIT_FIELD_BYTES_1`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum UnitAnimationTier {
    /// Ordinary ground animations.
    #[default]
    Ground = 0,
    /// Swimming tier; stock falls back directly to ground.
    Swim = 1,
    /// Hovering tier; stock falls back through the flying tier.
    Hover = 2,
    /// Flying animations.
    Fly = 3,
    /// Fully submerged animations; stock falls back directly to ground.
    Submerged = 4,
}

impl TryFrom<u8> for UnitAnimationTier {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ground),
            1 => Ok(Self::Swim),
            2 => Ok(Self::Hover),
            3 => Ok(Self::Fly),
            4 => Ok(Self::Submerged),
            other => Err(other),
        }
    }
}

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
    animation_tier: UnitAnimationTier,
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
        animation_tier: UnitAnimationTier,
        sheath_state: UnitSheathState,
    ) -> Self {
        Self {
            display_id,
            native_display_id,
            mount_display_id,
            stand_state,
            animation_tier,
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

    /// Returns byte three of `UNIT_FIELD_BYTES_1` as a closed stock tier.
    #[must_use]
    pub const fn animation_tier(self) -> UnitAnimationTier {
        self.animation_tier
    }

    /// Returns byte zero of `UNIT_FIELD_BYTES_2` as a closed stock state.
    #[must_use]
    pub const fn sheath_state(self) -> UnitSheathState {
        self.sheath_state
    }
}
