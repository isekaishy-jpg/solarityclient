//! Joins public player equipment fields to stock item display records.

use solarity_asset::{ItemDefinition, ItemDefinitionCatalog, ItemDisplayCatalog, ItemDisplayInfo};
use solarity_ecs::{
    ActiveWorld, ObjectKind, PlayerEquipment, PlayerEquipmentSlot, VisibleEquipmentItem,
};
use thiserror::Error;

/// One nonempty public slot resolved through `Item.dbc` and `ItemDisplayInfo.dbc`.
pub struct ResolvedEquipmentItem<'catalog> {
    slot: PlayerEquipmentSlot,
    visible: VisibleEquipmentItem,
    definition: &'catalog ItemDefinition,
    display: &'catalog ItemDisplayInfo,
}

impl ResolvedEquipmentItem<'_> {
    /// Returns the source public equipment slot.
    #[must_use]
    pub const fn slot(&self) -> PlayerEquipmentSlot {
        self.slot
    }

    /// Returns the exact entry/enchantment words received from the server.
    #[must_use]
    pub const fn visible(&self) -> VisibleEquipmentItem {
        self.visible
    }

    /// Returns the client item definition selected by the entry identifier.
    #[must_use]
    pub const fn definition(&self) -> &ItemDefinition {
        self.definition
    }

    /// Returns the client display record selected by the item definition.
    #[must_use]
    pub const fn display(&self) -> &ItemDisplayInfo {
        self.display
    }
}

/// All nonempty visible equipment for one player in public slot order.
pub struct PlayerEquipmentAppearance<'catalog> {
    guid: u64,
    items: Vec<ResolvedEquipmentItem<'catalog>>,
}

impl PlayerEquipmentAppearance<'_> {
    /// Returns the player GUID whose projected fields produced this appearance.
    #[must_use]
    pub const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns resolved nonempty slots in server order.
    #[must_use]
    pub fn items(&self) -> &[ResolvedEquipmentItem<'_>] {
        &self.items
    }
}

/// A missing ECS component or client table row needed for equipment display.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlayerEquipmentAppearanceError {
    /// The GUID is absent from the active-world registry.
    #[error("equipment appearance references unknown object {guid:#018X}")]
    UnknownObject {
        /// Unresolved server GUID.
        guid: u64,
    },
    /// The indexed object has not received its create-time category.
    #[error("equipment appearance references untyped object {guid:#018X}")]
    MissingObjectKind {
        /// Server GUID with incomplete create state.
        guid: u64,
    },
    /// Visible equipment fields are defined only for player objects.
    #[error("equipment appearance references non-player object {guid:#018X}")]
    NotPlayer {
        /// Server GUID with another object category.
        guid: u64,
    },
    /// The player's visible-item fields have not been projected.
    #[error("player {guid:#018X} has no visible equipment fields")]
    MissingEquipment {
        /// Player server GUID.
        guid: u64,
    },
    /// A nonzero public entry is absent from the client item table.
    #[error("player equipment slot {slot:?} references missing item entry {entry_id}")]
    MissingItem {
        /// Public player slot.
        slot: PlayerEquipmentSlot,
        /// Unresolved `Item.dbc` identifier.
        entry_id: u32,
    },
    /// An item definition references an absent display row.
    #[error(
        "player equipment slot {slot:?} item {entry_id} references missing display {display_id}"
    )]
    MissingDisplay {
        /// Public player slot.
        slot: PlayerEquipmentSlot,
        /// Source `Item.dbc` identifier.
        entry_id: u32,
        /// Unresolved `ItemDisplayInfo.dbc` identifier.
        display_id: u32,
    },
}

/// Resolves one player's nonempty visible slots without storing asset refs in ECS.
///
/// Zero entries remain empty slots. Every nonzero entry must resolve exactly;
/// another item or display row is never substituted.
///
/// # Errors
///
/// Returns [`PlayerEquipmentAppearanceError`] when the object or projected
/// component is absent, the object is not a player, or a required table row is missing.
pub fn resolve_player_equipment<'catalog>(
    world: &ActiveWorld,
    guid: u64,
    definitions: &'catalog ItemDefinitionCatalog,
    displays: &'catalog ItemDisplayCatalog,
) -> Result<PlayerEquipmentAppearance<'catalog>, PlayerEquipmentAppearanceError> {
    let entity = world
        .entity_by_guid(guid)
        .ok_or(PlayerEquipmentAppearanceError::UnknownObject { guid })?;
    let kind = world
        .storage()
        .get::<&ObjectKind>(entity)
        .map(|kind| **kind)
        .map_err(|_| PlayerEquipmentAppearanceError::MissingObjectKind { guid })?;
    if kind != ObjectKind::Player {
        return Err(PlayerEquipmentAppearanceError::NotPlayer { guid });
    }
    let equipment = world
        .storage()
        .get::<&PlayerEquipment>(entity)
        .map(|equipment| **equipment)
        .map_err(|_| PlayerEquipmentAppearanceError::MissingEquipment { guid })?;

    let mut resolved = Vec::with_capacity(PlayerEquipmentSlot::ALL.len());
    for slot in PlayerEquipmentSlot::ALL {
        let visible = equipment.item(slot);
        if visible.entry_id() == 0 {
            continue;
        }
        let definition = definitions.item(visible.entry_id()).ok_or(
            PlayerEquipmentAppearanceError::MissingItem {
                slot,
                entry_id: visible.entry_id(),
            },
        )?;
        let display_id = definition.display_info_id();
        let display =
            displays
                .display(display_id)
                .ok_or(PlayerEquipmentAppearanceError::MissingDisplay {
                    slot,
                    entry_id: visible.entry_id(),
                    display_id,
                })?;
        resolved.push(ResolvedEquipmentItem {
            slot,
            visible,
            definition,
            display,
        });
    }

    Ok(PlayerEquipmentAppearance {
        guid,
        items: resolved,
    })
}
