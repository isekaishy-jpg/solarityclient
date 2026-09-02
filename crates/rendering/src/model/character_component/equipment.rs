//! Render-bound item records for stock character composition.

use solarity_asset::{InventoryType, ItemDefinition, ItemDisplayInfo};
use solarity_ecs::{PlayerEquipmentSlot, VisibleEquipmentItem};

/// One resolved player item supplied to character render preparation.
#[derive(Clone, Copy, Debug)]
pub struct CharacterEquipmentItem<'catalog> {
    slot: PlayerEquipmentSlot,
    visible: VisibleEquipmentItem,
    definition: Option<&'catalog ItemDefinition>,
    display: &'catalog ItemDisplayInfo,
    inventory_type: Option<InventoryType>,
    item_visual_override: Option<u32>,
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
            inventory_type: Some(definition.inventory_type()),
            item_visual_override: None,
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
            inventory_type: Some(definition.inventory_type()),
            item_visual_override: None,
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
            inventory_type: None,
            item_visual_override: None,
        }
    }

    /// Creates one display-only character-enumeration input.
    #[must_use]
    pub const fn new_selection(
        slot: PlayerEquipmentSlot,
        display: &'catalog ItemDisplayInfo,
        inventory_type: InventoryType,
        item_visual_override: u32,
    ) -> Self {
        Self {
            slot,
            visible: VisibleEquipmentItem::new(0, 0),
            definition: None,
            display,
            inventory_type: Some(inventory_type),
            item_visual_override: if item_visual_override == 0 {
                None
            } else {
                Some(item_visual_override)
            },
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

    /// Returns the authoritative equipment category when supplied.
    #[must_use]
    pub const fn inventory_type(self) -> Option<InventoryType> {
        self.inventory_type
    }

    /// Returns the character-enumeration visual override when nonzero.
    #[must_use]
    pub const fn item_visual_override(self) -> Option<u32> {
        self.item_visual_override
    }

    /// Returns the item display supplying models, geosets, and texture stems.
    #[must_use]
    pub const fn display(self) -> &'catalog ItemDisplayInfo {
        self.display
    }
}
