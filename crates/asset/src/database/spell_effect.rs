//! Spell.dbc effect identifiers used by active aura and resurrection owners.

use super::{WdbcTable, localized::database_error};
use crate::{AssetError, AssetPath, AssetStore};
use std::collections::BTreeMap;

/// The authored effect triplets and resurrection attribute in build 12340.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpellEffectDefinition {
    /// Effect IDs from words 71–73, including self-resurrection effect 94.
    pub effects: [u32; 3],
    /// Aura type IDs from words 95–97.
    pub aura_types: [u32; 3],
    /// Effect misc values from words 110–112, including ScreenEffect IDs.
    pub misc_values: [u32; 3],
    /// Native internal Spell record word 11, bit 08000000.
    pub resurrection_bypass: bool,
}

/// Resident authored effect declarations, independent of active aura state.
pub struct SpellEffectCatalog {
    spells: BTreeMap<u32, SpellEffectDefinition>,
}

impl SpellEffectCatalog {
    /// Loads the exact build-12340 layout's effect and admission columns.
    ///
    /// # Errors
    /// Rejects missing data, a different schema, truncated records or duplicate IDs.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient/Spell.dbc")?)?;
        if table.header().field_count() != 234 || table.header().record_size() != 936 {
            return Err(database_error(
                &table,
                "expected build-12340 Spell.dbc with 234 four-byte fields".into(),
            ));
        }
        let mut spells = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let word = |column| {
                table.field_u32(row, column).ok_or_else(|| {
                    database_error(&table, format!("missing spell row {row} column {column}"))
                })
            };
            let id = word(0)?;
            let definition = SpellEffectDefinition {
                effects: [word(71)?, word(72)?, word(73)?],
                aura_types: [word(95)?, word(96)?, word(97)?],
                misc_values: [word(110)?, word(111)?, word(112)?],
                resurrection_bypass: word(11)? & 0x08000000 != 0,
            };
            if spells.insert(id, definition).is_some() {
                return Err(database_error(&table, format!("duplicate spell ID {id}")));
            }
        }
        Ok(Self { spells })
    }
    /// Returns a declaration only when its source Spell record exists.
    #[must_use]
    pub fn spell(&self, id: u32) -> Option<&SpellEffectDefinition> {
        self.spells.get(&id)
    }
}
