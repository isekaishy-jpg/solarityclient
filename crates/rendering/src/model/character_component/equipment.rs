//! Render-bound item records for stock character composition.

use solarity_asset::{ItemDefinition, ItemDisplayInfo};
use solarity_ecs::PlayerEquipmentSlot;

/// One resolved player item supplied to character render preparation.
#[derive(Clone, Copy, Debug)]
pub struct CharacterEquipmentItem<'catalog> {
    slot: PlayerEquipmentSlot,
    definition: &'catalog ItemDefinition,
    display: &'catalog ItemDisplayInfo,
}

impl<'catalog> CharacterEquipmentItem<'catalog> {
    /// Creates one render input after systems have resolved both client rows.
    #[must_use]
    pub const fn new(
        slot: PlayerEquipmentSlot,
        definition: &'catalog ItemDefinition,
        display: &'catalog ItemDisplayInfo,
    ) -> Self {
        Self {
            slot,
            definition,
            display,
        }
    }

    /// Returns the public equipment slot carrying the item.
    #[must_use]
    pub const fn slot(self) -> PlayerEquipmentSlot {
        self.slot
    }

    /// Returns the item definition used for category and attachment behavior.
    #[must_use]
    pub const fn definition(self) -> &'catalog ItemDefinition {
        self.definition
    }

    /// Returns the item display supplying models, geosets, and texture stems.
    #[must_use]
    pub const fn display(self) -> &'catalog ItemDisplayInfo {
        self.display
    }
}
