//! Localized character-class identities used by character-selection Glue.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::{database_error, localized_string};
use super::wow_client_db::WdbcTable;

const CHARACTER_CLASSES_PATH: &str = "DBFilesClient\\ChrClasses.dbc";
const CHARACTER_CLASSES_FIELD_COUNT: u32 = 60;
const LOCALIZED_NAME_FIRST_FIELD: u32 = 4;

/// One build-12340 character class and its selected-locale display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterClassDefinition {
    id: u32,
    name: String,
}

impl CharacterClassDefinition {
    /// Returns the protocol and ChrClasses identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the exact selected-locale class name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Identifier-indexed build-12340 character-class metadata.
pub struct CharacterClassCatalog {
    classes: Vec<CharacterClassDefinition>,
}

impl CharacterClassCatalog {
    /// Loads the exact 60-word ChrClasses layout.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, layout, text, or duplicate-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let path = AssetPath::new(CHARACTER_CLASSES_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;
        let mut classes = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            classes.push(CharacterClassDefinition {
                id: field(&table, row, 0)?,
                name: localized_string(&table, row, LOCALIZED_NAME_FIRST_FIELD, locale)?,
            });
        }
        classes.sort_unstable_by_key(CharacterClassDefinition::id);
        if let Some(duplicate) = classes.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { classes })
    }

    /// Finds one exact class identifier.
    #[must_use]
    pub fn class(&self, id: u32) -> Option<&CharacterClassDefinition> {
        self.classes
            .binary_search_by_key(&id, CharacterClassDefinition::id)
            .ok()
            .map(|index| &self.classes[index])
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == CHARACTER_CLASSES_FIELD_COUNT
        && header.record_size() == CHARACTER_CLASSES_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 ChrClasses.dbc requires 60 fields and 240-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}
