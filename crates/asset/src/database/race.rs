//! Exact build-12340 character race names used by component-model paths.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::wow_client_db::WdbcTable;

const CHARACTER_RACES_PATH: &str = "DBFilesClient\\ChrRaces.dbc";
const CHARACTER_RACES_FIELD_COUNT: u32 = 69;

/// One player race's stock file-naming and body-display identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterRace {
    id: u32,
    flags: u32,
    male_display_id: u32,
    female_display_id: u32,
    client_prefix: String,
    client_file_string: String,
}

impl CharacterRace {
    /// Returns the identifier carried by unit race bytes and appearance tables.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the complete build-12340 race flags word.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the male player creature-display identifier.
    #[must_use]
    pub const fn male_display_id(&self) -> u32 {
        self.male_display_id
    }

    /// Returns the female player creature-display identifier.
    #[must_use]
    pub const fn female_display_id(&self) -> u32 {
        self.female_display_id
    }

    /// Returns the short helmet-model suffix such as `Hu` or `Or`.
    #[must_use]
    pub fn client_prefix(&self) -> &str {
        &self.client_prefix
    }

    /// Returns the race's stock character-directory filename component.
    #[must_use]
    pub fn client_file_string(&self) -> &str {
        &self.client_file_string
    }
}

/// Identifier-indexed `ChrRaces.dbc` records used by character rendering.
pub struct CharacterRaceCatalog {
    races: Vec<CharacterRace>,
}

impl CharacterRaceCatalog {
    /// Loads the exact 69-word build-12340 race table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, contains invalid file strings, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(CHARACTER_RACES_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut races = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            races.push(CharacterRace {
                id: field(&table, row, 0)?,
                flags: field(&table, row, 1)?,
                male_display_id: field(&table, row, 4)?,
                female_display_id: field(&table, row, 5)?,
                client_prefix: string(&table, row, 6)?,
                client_file_string: string(&table, row, 11)?,
            });
        }
        races.sort_unstable_by_key(CharacterRace::id);
        if let Some(duplicate) = races.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { races })
    }

    /// Finds one exact race identifier without substituting another row.
    #[must_use]
    pub fn race(&self, id: u32) -> Option<&CharacterRace> {
        self.races
            .binary_search_by_key(&id, CharacterRace::id)
            .ok()
            .map(|index| &self.races[index])
    }
}

/// Rejects another build before interpreting sparse and localized columns.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == CHARACTER_RACES_FIELD_COUNT
        && header.record_size() == CHARACTER_RACES_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 ChrRaces.dbc requires 69 fields and 276-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one required physical field with record and column context.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Reads one ASCII file-naming string from the table string block.
fn string(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    if !bytes.is_ascii() || bytes.contains(&0) {
        return Err(database_error(
            table,
            format!("record {row} field {column} contains an invalid race file string"),
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
