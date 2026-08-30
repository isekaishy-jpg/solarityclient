//! Exact `Item.dbc` decoding for item-entry to display resolution.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const ITEM_PATH: &str = "DBFilesClient\\Item.dbc";
const ITEM_FIELD_COUNT: u32 = 8;

/// The stock inventory placement/category word used by character equipment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InventoryType {
    /// Not equippable.
    NonEquip = 0,
    /// Head.
    Head = 1,
    /// Neck.
    Neck = 2,
    /// Shoulders.
    Shoulders = 3,
    /// Shirt.
    Body = 4,
    /// Chest armor.
    Chest = 5,
    /// Waist.
    Waist = 6,
    /// Legs.
    Legs = 7,
    /// Feet.
    Feet = 8,
    /// Wrists.
    Wrists = 9,
    /// Hands.
    Hands = 10,
    /// Finger.
    Finger = 11,
    /// Trinket.
    Trinket = 12,
    /// One-handed weapon.
    Weapon = 13,
    /// Shield.
    Shield = 14,
    /// Ranged weapon.
    Ranged = 15,
    /// Cloak.
    Cloak = 16,
    /// Two-handed weapon.
    TwoHandWeapon = 17,
    /// Bag.
    Bag = 18,
    /// Tabard.
    Tabard = 19,
    /// Robe.
    Robe = 20,
    /// Main-hand-only weapon.
    MainHandWeapon = 21,
    /// Off-hand-only weapon.
    OffHandWeapon = 22,
    /// Held-in-off-hand object.
    Holdable = 23,
    /// Ammunition.
    Ammo = 24,
    /// Thrown weapon.
    Thrown = 25,
    /// Right-hand ranged weapon.
    RangedRight = 26,
    /// Quiver.
    Quiver = 27,
    /// Relic.
    Relic = 28,
}

impl TryFrom<u32> for InventoryType {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NonEquip),
            1 => Ok(Self::Head),
            2 => Ok(Self::Neck),
            3 => Ok(Self::Shoulders),
            4 => Ok(Self::Body),
            5 => Ok(Self::Chest),
            6 => Ok(Self::Waist),
            7 => Ok(Self::Legs),
            8 => Ok(Self::Feet),
            9 => Ok(Self::Wrists),
            10 => Ok(Self::Hands),
            11 => Ok(Self::Finger),
            12 => Ok(Self::Trinket),
            13 => Ok(Self::Weapon),
            14 => Ok(Self::Shield),
            15 => Ok(Self::Ranged),
            16 => Ok(Self::Cloak),
            17 => Ok(Self::TwoHandWeapon),
            18 => Ok(Self::Bag),
            19 => Ok(Self::Tabard),
            20 => Ok(Self::Robe),
            21 => Ok(Self::MainHandWeapon),
            22 => Ok(Self::OffHandWeapon),
            23 => Ok(Self::Holdable),
            24 => Ok(Self::Ammo),
            25 => Ok(Self::Thrown),
            26 => Ok(Self::RangedRight),
            27 => Ok(Self::Quiver),
            28 => Ok(Self::Relic),
            other => Err(other),
        }
    }
}

/// One exact build-12340 item definition used for visible presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    id: u32,
    class_id: u32,
    subclass_id: u32,
    sound_override_subclass_id: i32,
    material_id: i32,
    display_info_id: u32,
    inventory_type: InventoryType,
    sheathe_type: u32,
}

impl ItemDefinition {
    /// Returns the item entry identifier sent in player visible-item fields.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the item class identifier.
    #[must_use]
    pub const fn class_id(self) -> u32 {
        self.class_id
    }

    /// Returns the class-local subclass identifier.
    #[must_use]
    pub const fn subclass_id(self) -> u32 {
        self.subclass_id
    }

    /// Returns the signed sound override subclass, including stock's sentinel.
    #[must_use]
    pub const fn sound_override_subclass_id(self) -> i32 {
        self.sound_override_subclass_id
    }

    /// Returns the signed material identifier.
    #[must_use]
    pub const fn material_id(self) -> i32 {
        self.material_id
    }

    /// Returns the referenced `ItemDisplayInfo.dbc` identifier.
    #[must_use]
    pub const fn display_info_id(self) -> u32 {
        self.display_info_id
    }

    /// Returns the stock equipment category.
    #[must_use]
    pub const fn inventory_type(self) -> InventoryType {
        self.inventory_type
    }

    /// Returns the exact weapon sheathe category word.
    #[must_use]
    pub const fn sheathe_type(self) -> u32 {
        self.sheathe_type
    }
}

/// Identifier-indexed build-12340 item definitions.
pub struct ItemDefinitionCatalog {
    items: Vec<ItemDefinition>,
}

impl ItemDefinitionCatalog {
    /// Loads the exact eight-field client item table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, repeats an identifier, or contains an invalid inventory type.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(ITEM_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut items = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let fields = read_fields(&table, row)?;
            let inventory_type = InventoryType::try_from(fields[6]).map_err(|value| {
                database_error(
                    &table,
                    format!("record {row} has invalid inventory type {value}"),
                )
            })?;
            items.push(ItemDefinition {
                id: fields[0],
                class_id: fields[1],
                subclass_id: fields[2],
                sound_override_subclass_id: fields[3] as i32,
                material_id: fields[4] as i32,
                display_info_id: fields[5],
                inventory_type,
                sheathe_type: fields[7],
            });
        }
        items.sort_unstable_by_key(|item| item.id);
        if let Some(duplicate) = items.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { items })
    }

    /// Finds one exact item entry without substituting server-side cache data.
    #[must_use]
    pub fn item(&self, id: u32) -> Option<&ItemDefinition> {
        self.items
            .binary_search_by_key(&id, |item| item.id)
            .ok()
            .map(|index| &self.items[index])
    }
}

/// Rejects another build's item row width.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == ITEM_FIELD_COUNT && header.record_size() == ITEM_FIELD_COUNT * 4 {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 Item.dbc requires 8 fields and 32-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one exact item row through checked table access.
fn read_fields(table: &WdbcTable, row: u32) -> Result<[u32; 8], AssetError> {
    let mut fields = [0_u32; 8];
    for (field, value) in fields.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(fields)
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
