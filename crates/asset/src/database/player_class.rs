//! Localized character-class identities used by character-selection Glue.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::{database_error, localized_string};
use super::wow_client_db::WdbcTable;

const CHARACTER_CLASSES_PATH: &str = "DBFilesClient\\ChrClasses.dbc";
const CHARACTER_CLASSES_FIELD_COUNT: u32 = 60;
const LOCALIZED_NAME_FIRST_FIELD: u32 = 4;
const LOCALIZED_FEMALE_NAME_FIRST_FIELD: u32 = 21;
const LOCALIZED_MALE_NAME_FIRST_FIELD: u32 = 38;
const FILE_STRING_FIELD: u32 = 55;

/// One build-12340 character class and its selected-locale display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterClassDefinition {
    id: u32,
    name: String,
    female_name: String,
    male_name: String,
    file_string: String,
    required_expansion: u32,
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

    /// Returns the exact selected-locale female class name.
    #[must_use]
    pub fn female_name(&self) -> &str {
        &self.female_name
    }

    /// Returns the exact selected-locale male class name.
    #[must_use]
    pub fn male_name(&self) -> &str {
        &self.male_name
    }

    /// Returns the stock class filename token before API-specific casing.
    #[must_use]
    pub fn file_string(&self) -> &str {
        &self.file_string
    }

    /// Returns the minimum account expansion required to select the class.
    #[must_use]
    pub const fn required_expansion(&self) -> u32 {
        self.required_expansion
    }

    /// Selects the exact stock display-name column for one binary gender.
    #[must_use]
    pub fn display_name(&self, gender_id: u8) -> &str {
        let authored = match gender_id {
            0 => &self.male_name,
            1 => &self.female_name,
            _ => "",
        };
        if authored.is_empty() {
            &self.name
        } else {
            authored
        }
    }
}

/// Identifier-indexed build-12340 character-class metadata.
pub struct CharacterClassCatalog {
    classes: Vec<CharacterClassDefinition>,
    physical_order: Vec<usize>,
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
                female_name: localized_string(
                    &table,
                    row,
                    LOCALIZED_FEMALE_NAME_FIRST_FIELD,
                    locale,
                )?,
                male_name: localized_string(&table, row, LOCALIZED_MALE_NAME_FIRST_FIELD, locale)?,
                file_string: string(&table, row, FILE_STRING_FIELD)?,
                required_expansion: field(&table, row, 59)?,
            });
        }
        let physical_ids = classes
            .iter()
            .map(CharacterClassDefinition::id)
            .collect::<Vec<_>>();
        classes.sort_unstable_by_key(CharacterClassDefinition::id);
        if let Some(duplicate) = classes.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        let physical_order = physical_ids
            .into_iter()
            .map(|id| {
                classes
                    .binary_search_by_key(&id, CharacterClassDefinition::id)
                    .map_err(|_source| database_error(&table, format!("lost class key {id}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            classes,
            physical_order,
        })
    }

    /// Finds one exact class identifier.
    #[must_use]
    pub fn class(&self, id: u32) -> Option<&CharacterClassDefinition> {
        self.classes
            .binary_search_by_key(&id, CharacterClassDefinition::id)
            .ok()
            .map(|index| &self.classes[index])
    }

    /// Iterates classes in ascending protocol-identifier order.
    pub fn classes(&self) -> impl ExactSizeIterator<Item = &CharacterClassDefinition> {
        self.classes.iter()
    }

    /// Iterates records in physical DBC order, matching stock creation globals.
    pub fn physical_classes(&self) -> impl ExactSizeIterator<Item = &CharacterClassDefinition> {
        self.physical_order
            .iter()
            .map(|index| &self.classes[*index])
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
            format!("record {row} field {column} contains an invalid class file string"),
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}
