//! Exact build-12340 realm rule-set configuration records.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const REALM_CONFIGURATIONS_PATH: &str = "DBFilesClient\\Cfg_Configs.dbc";
const REALM_CONFIGURATIONS_FIELD_COUNT: u32 = 4;

/// One client-authored realm rule set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealmConfiguration {
    id: u32,
    realm_type: u32,
    player_killing_allowed: bool,
    roleplaying: bool,
}

impl RealmConfiguration {
    /// Returns the configuration identifier referenced by realm categories.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the stock numeric realm-type identifier.
    #[must_use]
    pub const fn realm_type(self) -> u32 {
        self.realm_type
    }

    /// Reports whether the configuration permits player killing.
    #[must_use]
    pub const fn player_killing_allowed(self) -> bool {
        self.player_killing_allowed
    }

    /// Reports whether the configuration is designated for roleplaying.
    #[must_use]
    pub const fn roleplaying(self) -> bool {
        self.roleplaying
    }
}

/// Identifier-indexed `Cfg_Configs.dbc` records used by realm presentation.
pub struct RealmConfigurationCatalog {
    configurations: Vec<RealmConfiguration>,
}

impl RealmConfigurationCatalog {
    /// Loads the exact four-field build-12340 configuration table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, contains a non-Boolean flag, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(REALM_CONFIGURATIONS_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut configurations = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            configurations.push(RealmConfiguration {
                id: field(&table, row, 0)?,
                realm_type: field(&table, row, 1)?,
                player_killing_allowed: boolean_field(&table, row, 2)?,
                roleplaying: boolean_field(&table, row, 3)?,
            });
        }
        configurations.sort_unstable_by_key(|configuration| configuration.id);
        if let Some(duplicate) = configurations
            .windows(2)
            .find(|pair| pair[0].id == pair[1].id)
        {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }

        Ok(Self { configurations })
    }

    /// Returns all authored configurations in identifier order.
    pub fn configurations(&self) -> impl ExactSizeIterator<Item = &RealmConfiguration> {
        self.configurations.iter()
    }

    /// Finds one exact configuration identifier without substituting another row.
    #[must_use]
    pub fn configuration(&self, id: u32) -> Option<&RealmConfiguration> {
        self.configurations
            .binary_search_by_key(&id, |configuration| configuration.id)
            .ok()
            .map(|index| &self.configurations[index])
    }
}

/// Rejects another build before interpreting the four physical columns.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == REALM_CONFIGURATIONS_FIELD_COUNT
        && header.record_size() == REALM_CONFIGURATIONS_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 Cfg_Configs.dbc requires 4 fields and 16-byte records; found {} fields and {}-byte records",
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

/// Accepts only the binary values authored by the stock Boolean columns.
fn boolean_field(table: &WdbcTable, row: u32, column: u32) -> Result<bool, AssetError> {
    match field(table, row, column)? {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(database_error(
            table,
            format!("record {row} field {column} has non-Boolean value {value}"),
        )),
    }
}

/// Adds the selected archive path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
