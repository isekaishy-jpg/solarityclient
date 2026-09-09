//! The six extShadowQuality values consumed by original 874210 and 875D30.

/// Stock exterior-shadow quality, including the disabled value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldShadowQuality {
    /// Authored terrain masks without dynamic shadow maps.
    Disabled,
    /// A 1024-square map for player and creature shadows.
    UnitsLow,
    /// A 2048-square map for player and creature shadows.
    UnitsHigh,
    /// Unit and environment shadows using 1024-square maps.
    EnvironmentLow,
    /// Unit and environment shadows using 2048-square maps.
    EnvironmentHigh,
    /// Stock's highest-quality cascaded environment policy.
    Cascaded,
}

impl WorldShadowQuality {
    /// Decodes the exact CVar range; values outside zero through five are invalid.
    #[must_use]
    pub const fn from_cvar(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::UnitsLow),
            2 => Some(Self::UnitsHigh),
            3 => Some(Self::EnvironmentLow),
            4 => Some(Self::EnvironmentHigh),
            5 => Some(Self::Cascaded),
            _ => None,
        }
    }

    /// Returns the square texture extent allocated by original 875D30.
    #[must_use]
    pub const fn texture_size(self) -> Option<u32> {
        match self {
            Self::Disabled => None,
            Self::UnitsLow | Self::EnvironmentLow => Some(1024),
            Self::UnitsHigh | Self::EnvironmentHigh | Self::Cascaded => Some(2048),
        }
    }

    /// Returns the shader mode selected through original B1D554 at 873FF0.
    #[must_use]
    pub const fn shader_mode(self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::UnitsLow | Self::UnitsHigh => 1,
            Self::EnvironmentLow | Self::EnvironmentHigh => 2,
            Self::Cascaded => 3,
        }
    }
}
