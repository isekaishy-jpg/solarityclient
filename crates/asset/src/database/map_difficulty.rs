//! Transfer-denial text from build-12340 MapDifficulty.dbc.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::{database_error, localized_string};
use super::wow_client_db::WdbcTable;

/// Exact map/difficulty messages consumed by transfer-abort handler 0x00403910.
pub struct MapDifficultyCatalog {
    messages: Vec<MapDifficultyMessage>,
}

/// One selected-locale message in original table order.
struct MapDifficultyMessage {
    map_id: u32,
    difficulty: u32,
    text: String,
}

impl MapDifficultyCatalog {
    /// Loads the stock 23-word layout, reducing its localized message at field 3.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for missing assets, invalid layout, or malformed text.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient\\MapDifficulty.dbc")?)?;
        if table.header().field_count() != 23 || table.header().record_size() != 92 {
            return Err(database_error(
                &table,
                "build-12340 MapDifficulty.dbc requires 23 fields and 92-byte records".to_owned(),
            ));
        }
        let mut messages = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            messages.push(MapDifficultyMessage {
                map_id: field(&table, row, 1)?,
                difficulty: field(&table, row, 2)?,
                text: localized_string(&table, row, 3, locale)?,
            });
        }
        Ok(Self { messages })
    }

    /// Returns the first exact map/difficulty match, including an empty message.
    ///
    /// Stock 0x00634950 searches the map's contiguous record run. There is no
    /// difficulty substitution on the transfer-abort call's null third argument.
    #[must_use]
    pub fn message(&self, map_id: u32, difficulty: u32) -> Option<&str> {
        let start = self
            .messages
            .iter()
            .position(|message| message.map_id == map_id)?;
        self.messages[start..]
            .iter()
            .take_while(|message| message.map_id == map_id)
            .find(|message| message.difficulty == difficulty)
            .map(|message| message.text.as_str())
    }
}

/// Reads one required fixed-layout field with a table-local error.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}
