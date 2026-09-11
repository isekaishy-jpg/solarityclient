//! Replicated Unit_C held-item entries, independent of player visible armor.

use shipyard::Component;

/// Item.dbc entries from build-12340 `UNIT_VIRTUAL_ITEM_SLOT_ID` words 56..58.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitVirtualItems {
    entries: [u32; 3],
}

impl UnitVirtualItems {
    /// Creates the authoritative main-hand, off-hand, and ranged item entries.
    #[must_use]
    pub const fn new(entries: [u32; 3]) -> Self {
        Self { entries }
    }

    /// Returns main-hand, off-hand, and ranged entries; zero means empty.
    #[must_use]
    pub const fn entries(self) -> [u32; 3] {
        self.entries
    }
}
