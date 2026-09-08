//! Native environmental classifications consumed by combat presentation.

/// The six indexed environmental categories in build 12340.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EnvironmentalDamageKind {
    /// Exhaustion beyond safe water.
    Fatigue,
    /// Depleted underwater breath.
    Drowning,
    /// Ground impact after falling.
    Falling,
    /// Lava contact.
    Lava,
    /// Slime contact.
    Slime,
    /// Environmental fire contact.
    Fire,
}

impl EnvironmentalDamageKind {
    /// Resolves a valid native table index without reading past its six rows.
    #[must_use]
    pub const fn from_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Fatigue),
            1 => Some(Self::Drowning),
            2 => Some(Self::Falling),
            3 => Some(Self::Lava),
            4 => Some(Self::Slime),
            5 => Some(Self::Fire),
            _ => None,
        }
    }

    /// Returns the original case-sensitive combat-log token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Fatigue => "FATIGUE",
            Self::Drowning => "DROWNING",
            Self::Falling => "FALLING",
            Self::Lava => "LAVA",
            Self::Slime => "SLIME",
            Self::Fire => "FIRE",
        }
    }

    /// Returns the school mask from native A2D394.
    #[must_use]
    pub const fn school(self) -> u32 {
        match self {
            Self::Fatigue | Self::Drowning | Self::Falling => 1,
            Self::Lava | Self::Fire => 4,
            Self::Slime => 8,
        }
    }
}
