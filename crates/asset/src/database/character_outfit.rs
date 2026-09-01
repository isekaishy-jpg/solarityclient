//! Packed build-12340 character-creation starter outfits.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::wow_client_db::WdbcTable;

const CHARACTER_START_OUTFIT_PATH: &str = "DBFilesClient\\CharStartOutfit.dbc";
const CHARACTER_START_OUTFIT_FIELD_COUNT: u32 = 77;
const CHARACTER_START_OUTFIT_RECORD_SIZE: u32 = 296;
const CHARACTER_START_OUTFIT_ITEM_COUNT: usize = 24;

/// One parallel item/display/inventory-type entry in a starter outfit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterStartOutfitItem {
    item_id: i32,
    display_info_id: i32,
    inventory_type: i32,
}

impl CharacterStartOutfitItem {
    /// Returns the item identifier, including zero and stock's `-1` sentinel.
    #[must_use]
    pub const fn item_id(self) -> i32 {
        self.item_id
    }

    /// Returns the display identifier, including stock's `-1` sentinel.
    #[must_use]
    pub const fn display_info_id(self) -> i32 {
        self.display_info_id
    }

    /// Returns the inventory-type word, including stock's `-1` sentinel.
    #[must_use]
    pub const fn inventory_type(self) -> i32 {
        self.inventory_type
    }
}

/// One physical `CharStartOutfit.dbc` row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterStartOutfit {
    id: u32,
    race_id: u8,
    class_id: u8,
    gender_id: u8,
    outfit_id: u8,
    items: [CharacterStartOutfitItem; CHARACTER_START_OUTFIT_ITEM_COUNT],
}

impl CharacterStartOutfit {
    /// Returns the table primary key.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the protocol race identifier.
    #[must_use]
    pub const fn race_id(self) -> u8 {
        self.race_id
    }

    /// Returns the protocol class identifier.
    #[must_use]
    pub const fn class_id(self) -> u8 {
        self.class_id
    }

    /// Returns the playable gender identifier.
    #[must_use]
    pub const fn gender_id(self) -> u8 {
        self.gender_id
    }

    /// Returns the authored outfit variant byte.
    #[must_use]
    pub const fn outfit_id(self) -> u8 {
        self.outfit_id
    }

    /// Returns all twenty-four parallel starter-item entries in physical order.
    #[must_use]
    pub const fn items(&self) -> &[CharacterStartOutfitItem; CHARACTER_START_OUTFIT_ITEM_COUNT] {
        &self.items
    }
}

/// Physical-order character-creation starter outfits.
pub struct CharacterStartOutfitCatalog {
    outfits: Vec<CharacterStartOutfit>,
}

impl CharacterStartOutfitCatalog {
    /// Loads the exact packed build-12340 table.
    ///
    /// The first ordinary word is the row identifier. Race, class, gender,
    /// and outfit are four packed bytes; three parallel arrays then carry
    /// item, display, and inventory-type values.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, has another build's
    /// layout, contains an invalid playable key, or repeats that key.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(CHARACTER_START_OUTFIT_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        let header = table.header();
        if header.field_count() != CHARACTER_START_OUTFIT_FIELD_COUNT
            || header.record_size() != CHARACTER_START_OUTFIT_RECORD_SIZE
        {
            return Err(database_error(
                &table,
                format!(
                    "build-12340 CharStartOutfit.dbc requires 77 fields and 296-byte records; found {} fields and {}-byte records",
                    header.field_count(),
                    header.record_size()
                ),
            ));
        }

        let mut outfits = Vec::with_capacity(header.record_count() as usize);
        for row in 0..header.record_count() {
            let record = table
                .record(row)
                .ok_or_else(|| database_error(&table, format!("record {row} is truncated")))?;
            let id = read_u32(record, 0).ok_or_else(|| {
                database_error(&table, format!("record {row} identifier is truncated"))
            })?;
            let packed = record.get(4..8).ok_or_else(|| {
                database_error(&table, format!("record {row} key bytes are truncated"))
            })?;
            let race_id = packed[0];
            let class_id = packed[1];
            let gender_id = packed[2];
            let outfit_id = packed[3];
            if id == 0 || race_id == 0 || class_id == 0 || gender_id > 1 {
                return Err(database_error(
                    &table,
                    format!(
                        "record {row} has invalid id/race/class/gender {id}/{race_id}/{class_id}/{gender_id}"
                    ),
                ));
            }

            let mut items = [CharacterStartOutfitItem {
                item_id: -1,
                display_info_id: -1,
                inventory_type: -1,
            }; CHARACTER_START_OUTFIT_ITEM_COUNT];
            for (index, item) in items.iter_mut().enumerate() {
                item.item_id = read_i32(record, 8 + index * 4).ok_or_else(|| {
                    database_error(&table, format!("record {row} item {index} is truncated"))
                })?;
                item.display_info_id =
                    read_i32(record, 8 + (CHARACTER_START_OUTFIT_ITEM_COUNT + index) * 4)
                        .ok_or_else(|| {
                            database_error(
                                &table,
                                format!("record {row} display {index} is truncated"),
                            )
                        })?;
                item.inventory_type = read_i32(
                    record,
                    8 + (CHARACTER_START_OUTFIT_ITEM_COUNT * 2 + index) * 4,
                )
                .ok_or_else(|| {
                    database_error(
                        &table,
                        format!("record {row} inventory type {index} is truncated"),
                    )
                })?;
            }
            let outfit = CharacterStartOutfit {
                id,
                race_id,
                class_id,
                gender_id,
                outfit_id,
                items,
            };
            if outfits.iter().any(|existing: &CharacterStartOutfit| {
                existing.race_id == race_id
                    && existing.class_id == class_id
                    && existing.gender_id == gender_id
            }) {
                return Err(database_error(
                    &table,
                    format!("duplicate race/class/gender key {race_id}/{class_id}/{gender_id}"),
                ));
            }
            outfits.push(outfit);
        }
        Ok(Self { outfits })
    }

    /// Returns all rows in physical DBC order.
    #[must_use]
    pub fn outfits(&self) -> &[CharacterStartOutfit] {
        &self.outfits
    }

    /// Finds the stock outfit for one playable race/class/gender key.
    #[must_use]
    pub fn outfit(
        &self,
        race_id: u8,
        class_id: u8,
        gender_id: u8,
    ) -> Option<&CharacterStartOutfit> {
        self.outfits.iter().find(|outfit| {
            outfit.race_id == race_id
                && outfit.class_id == class_id
                && outfit.gender_id == gender_id
        })
    }
}

fn read_u32(record: &[u8], offset: usize) -> Option<u32> {
    let bytes = record.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_i32(record: &[u8], offset: usize) -> Option<i32> {
    read_u32(record, offset).map(|value| value as i32)
}

fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
