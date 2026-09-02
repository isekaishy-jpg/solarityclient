//! Build-12340 script-name lookups for interface sound kits.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const UI_SOUND_LOOKUPS_PATH: &str = "DBFilesClient\\UISoundLookups.dbc";
const UI_SOUND_LOOKUPS_FIELD_COUNT: u32 = 3;

/// One exact `UISoundLookups.dbc` row addressed by `PlaySound`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSoundLookup {
    id: u32,
    sound_entry_id: u32,
    name: String,
}

impl UiSoundLookup {
    /// Returns the table's primary identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the related `SoundEntries.dbc` identifier.
    #[must_use]
    pub const fn sound_entry_id(&self) -> u32 {
        self.sound_entry_id
    }

    /// Returns the unprefixed script lookup name authored by the table.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Name-addressable build-12340 interface-sound lookup table.
pub struct UiSoundLookupCatalog {
    entries: Vec<UiSoundLookup>,
}

impl UiSoundLookupCatalog {
    /// Loads the exact three-word table through ordinary archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, layout, string, or duplicate-primary-
    /// key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(UI_SOUND_LOOKUPS_PATH)?)?;
        require_layout(&table)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            entries.push(UiSoundLookup {
                id: field(&table, row, 0)?,
                sound_entry_id: field(&table, row, 1)?,
                name: string(&table, row, 2)?,
            });
        }
        entries.sort_unstable_by_key(UiSoundLookup::id);
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds the case-insensitive name used by the stock sound-name hash.
    #[must_use]
    pub fn entry_by_name(&self, name: &str) -> Option<&UiSoundLookup> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Returns every row in ascending primary-key order.
    #[must_use]
    pub fn entries(&self) -> &[UiSoundLookup] {
        &self.entries
    }
}

/// Enforces the 3.3.5a table shape recovered from build 12340.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == UI_SOUND_LOOKUPS_FIELD_COUNT
        && header.record_size() == UI_SOUND_LOOKUPS_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 UISoundLookups.dbc requires 3 fields and 12-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one required fixed-width field without hiding truncated input.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Resolves one required UTF-8 table string.
fn string(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|source| database_error(table, source.to_string()))
}
