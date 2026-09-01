//! Render-bound item records for stock character composition.

use solarity_asset::{ItemDefinition, ItemDisplayInfo};
use solarity_ecs::{PlayerEquipmentSlot, VisibleEquipmentItem};

/// One resolved player item supplied to character render preparation.
#[derive(Clone, Copy, Debug)]
pub struct CharacterEquipmentItem<'catalog> {
    slot: PlayerEquipmentSlot,
    visible: VisibleEquipmentItem,
    definition: Option<&'catalog ItemDefinition>,
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
            visible: VisibleEquipmentItem::new(0, 0),
            definition: Some(definition),
            display,
        }
    }

    /// Creates one render input retaining the exact public visible-item pair.
    #[must_use]
    pub const fn new_visible(
        slot: PlayerEquipmentSlot,
        visible: VisibleEquipmentItem,
        definition: &'catalog ItemDefinition,
        display: &'catalog ItemDisplayInfo,
    ) -> Self {
        Self {
            slot,
            visible,
            definition: Some(definition),
            display,
        }
    }

    /// Creates one display-only armor input from `CreatureDisplayInfoExtra`.
    ///
    /// NPC equipment stores display identifiers directly and has no Item.dbc
    /// row. Those eleven slots contain armor only, so weapon categorization is
    /// neither required nor inferred.
    #[must_use]
    pub const fn new_npc(slot: PlayerEquipmentSlot, display: &'catalog ItemDisplayInfo) -> Self {
        Self {
            slot,
            visible: VisibleEquipmentItem::new(0, 0),
            definition: None,
            display,
        }
    }

    /// Returns the public equipment slot carrying the item.
    #[must_use]
    pub const fn slot(self) -> PlayerEquipmentSlot {
        self.slot
    }

    /// Returns the exact public item fields that selected this render input.
    #[must_use]
    pub const fn visible(self) -> VisibleEquipmentItem {
        self.visible
    }

    /// Returns the item definition used for category and attachment behavior.
    #[must_use]
    pub const fn definition(self) -> Option<&'catalog ItemDefinition> {
        self.definition
    }

    /// Returns the item display supplying models, geosets, and texture stems.
    #[must_use]
    pub const fn display(self) -> &'catalog ItemDisplayInfo {
        self.display
    }
}
