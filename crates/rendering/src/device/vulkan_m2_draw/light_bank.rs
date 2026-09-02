//! Stock Glue light-bank identity retained through unified M2 submission.

/// Selects which authored Glue light bank illuminates an M2 draw.
///
/// World models and Glue environment models use [`Self::Environment`]. Glue
/// character bodies, equipment, and item visuals use [`Self::Character`],
/// while the character-selection companion uses [`Self::Pet`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum M2SceneLightBank {
    /// Environment and ordinary world lighting.
    #[default]
    Environment,
    /// Character-preview lighting authored through `AddCharacterLight`.
    Character,
    /// Companion-preview lighting authored through `AddPetLight`.
    Pet,
}

impl M2SceneLightBank {
    /// Number of stock light banks represented in one unified frame.
    pub(crate) const COUNT: usize = 3;

    /// Returns the stable descriptor position assigned to this stock bank.
    pub(in crate::device) const fn index(self) -> usize {
        match self {
            Self::Environment => 0,
            Self::Character => 1,
            Self::Pet => 2,
        }
    }
}
