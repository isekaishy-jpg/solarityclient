//! Packed race/class combinations admitted by build-12340 character creation.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::wow_client_db::WdbcTable;

const CHARACTER_BASE_INFO_PATH: &str = "DBFilesClient\\CharBaseInfo.dbc";

/// One physical two-byte `CharBaseInfo.dbc` row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterBaseInfo {
    race_id: u8,
    class_id: u8,
}

impl CharacterBaseInfo {
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
}

/// Physical-order character-creation race/class combinations.
pub struct CharacterBaseCatalog {
    entries: Vec<CharacterBaseInfo>,
}

impl CharacterBaseCatalog {
    /// Loads the exact packed two-column build-12340 table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, has another build's
    /// layout, contains zero identifiers, or repeats a race/class pair.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(CHARACTER_BASE_INFO_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        let header = table.header();
        if header.field_count() != 2 || header.record_size() != 2 {
            return Err(database_error(
                &table,
                format!(
                    "build-12340 CharBaseInfo.dbc requires 2 fields and 2-byte records; found {} fields and {}-byte records",
                    header.field_count(),
                    header.record_size()
                ),
            ));
        }

        let mut entries = Vec::with_capacity(header.record_count() as usize);
        for row in 0..header.record_count() {
            let race_id = table.field_u8(row, 0).ok_or_else(|| {
                database_error(&table, format!("record {row} race byte is truncated"))
            })?;
            let class_id = table.field_u8(row, 1).ok_or_else(|| {
                database_error(&table, format!("record {row} class byte is truncated"))
            })?;
            if race_id == 0 || class_id == 0 {
                return Err(database_error(
                    &table,
                    format!("record {row} contains a zero race or class identifier"),
                ));
            }
            let entry = CharacterBaseInfo { race_id, class_id };
            if entries.contains(&entry) {
                return Err(database_error(
                    &table,
                    format!("duplicate race/class pair {race_id}/{class_id}"),
                ));
            }
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    /// Returns rows in physical DBC order, matching stock `CalcClasses`.
    #[must_use]
    pub fn entries(&self) -> &[CharacterBaseInfo] {
        &self.entries
    }

    /// Reports whether the exact protocol pair is authored by the client.
    #[must_use]
    pub fn supports(&self, race_id: u8, class_id: u8) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.race_id == race_id && entry.class_id == class_id)
    }
}

fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
