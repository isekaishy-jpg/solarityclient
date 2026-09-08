//! Localized spell names used by native mirror-timer labels.

use std::collections::BTreeMap;

use crate::{AssetError, AssetPath, AssetStore};

use super::{
    WdbcTable,
    localized::{database_error, localized_string},
};

/// Selected-locale names from the exact build-12340 `Spell.dbc` layout.
pub struct SpellNameCatalog {
    names: BTreeMap<u32, String>,
}

impl SpellNameCatalog {
    /// Loads the spell-name column without interpreting unrelated spell behavior.
    ///
    /// # Errors
    /// Rejects missing data, a different schema, invalid localized strings, or duplicate IDs.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient\\Spell.dbc")?)?;
        if table.header().field_count() != 234 || table.header().record_size() != 936 {
            return Err(database_error(
                &table,
                "expected build-12340 Spell.dbc with 234 four-byte fields".into(),
            ));
        }
        let mut names = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let id = table
                .field_u32(row, 0)
                .ok_or_else(|| database_error(&table, format!("missing spell ID at row {row}")))?;
            let name = localized_string(&table, row, 136, locale)?;
            if names.insert(id, name).is_some() {
                return Err(database_error(&table, format!("duplicate spell ID {id}")));
            }
        }
        Ok(Self { names })
    }

    /// Returns the exact localized name, including an authored empty name.
    #[must_use]
    pub fn name(&self, id: u32) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }
}
