//! Exact build-12340 realm-category names and regional visibility masks.

use crate::archive::{AssetError, AssetPath, Locale};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const REALM_CATEGORIES_PATH: &str = "DBFilesClient\\Cfg_Categories.dbc";
const REALM_CATEGORIES_FIELD_COUNT: u32 = 21;
const LOCALIZED_NAME_FIRST_FIELD: u32 = 4;
const LOCALIZED_NAME_FLAGS_FIELD: u32 = 20;

/// One stock realm-list category and its localized display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealmCategoryDefinition {
    id: u32,
    locale_mask: u32,
    character_set_mask: u32,
    flags: u32,
    name: String,
    name_flags: u32,
}

impl RealmCategoryDefinition {
    /// Returns the category identifier carried by the logon realm list.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the stock client-locale visibility mask.
    #[must_use]
    pub const fn locale_mask(&self) -> u32 {
        self.locale_mask
    }

    /// Returns the stock character-creation character-set mask.
    #[must_use]
    pub const fn character_set_mask(&self) -> u32 {
        self.character_set_mask
    }

    /// Returns the category behavior flags word.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the display name from the mounted client's exact locale slot.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the trailing localized-string flags word.
    #[must_use]
    pub const fn name_flags(&self) -> u32 {
        self.name_flags
    }
}

/// Identifier-indexed `Cfg_Categories.dbc` records used by the realm-list UI.
pub struct RealmCategoryCatalog {
    categories: Vec<RealmCategoryDefinition>,
}

impl RealmCategoryCatalog {
    /// Loads the exact 21-field build-12340 category table.
    ///
    /// The selected name column is determined solely by the mounted archive
    /// locale. Empty locale fields remain empty; another language is never
    /// substituted implicitly.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, contains invalid UTF-8, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let path = AssetPath::new(REALM_CATEGORIES_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let name_field = LOCALIZED_NAME_FIRST_FIELD + localized_string_index(locale);
        let mut categories = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            categories.push(RealmCategoryDefinition {
                id: field(&table, row, 0)?,
                locale_mask: field(&table, row, 1)?,
                character_set_mask: field(&table, row, 2)?,
                flags: field(&table, row, 3)?,
                name: localized_name(&table, row, name_field)?,
                name_flags: field(&table, row, LOCALIZED_NAME_FLAGS_FIELD)?,
            });
        }
        categories.sort_unstable_by_key(RealmCategoryDefinition::id);
        if let Some(duplicate) = categories.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }

        Ok(Self { categories })
    }

    /// Returns all authored category rows in identifier order.
    pub fn categories(&self) -> impl ExactSizeIterator<Item = &RealmCategoryDefinition> {
        self.categories.iter()
    }

    /// Finds one exact category identifier without substituting another row.
    #[must_use]
    pub fn category(&self, id: u32) -> Option<&RealmCategoryDefinition> {
        self.categories
            .binary_search_by_key(&id, RealmCategoryDefinition::id)
            .ok()
            .map(|index| &self.categories[index])
    }
}

/// Maps build-12340 locale tokens to their physical locstring columns.
///
/// English regional archive tokens share the authored enUS language column;
/// this is the client schema's aliasing rule, not a missing-string fallback.
const fn localized_string_index(locale: Locale) -> u32 {
    match locale {
        Locale::EnUs | Locale::EnGb | Locale::EnCn | Locale::EnTw => 0,
        Locale::KoKr => 1,
        Locale::FrFr => 2,
        Locale::DeDe => 3,
        Locale::ZhCn => 4,
        Locale::ZhTw => 5,
        Locale::EsEs => 6,
        Locale::EsMx => 7,
        Locale::RuRu => 8,
    }
}

/// Rejects another build before interpreting the expanded locstring columns.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == REALM_CATEGORIES_FIELD_COUNT
        && header.record_size() == REALM_CATEGORIES_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 Cfg_Categories.dbc requires 21 fields and 84-byte records; found {} fields and {}-byte records",
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

/// Decodes the exact selected locale slot as the client's UTF-8 text.
fn localized_name(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|error| {
        database_error(
            table,
            format!("record {row} field {column} is not UTF-8: {error}"),
        )
    })
}

/// Adds the selected archive path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
