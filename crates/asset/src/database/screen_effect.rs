//! Authored ScreenEffect.dbc declarations consumed by the world view.

use super::{WdbcTable, WorldLightCondition, localized::database_error};
use crate::{AssetError, AssetPath, AssetStore};
use std::collections::HashMap;

/// One exact build-12340 screen-effect row, including its raw filter arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenEffectDefinition {
    /// Authored effect type: normal, ghost, invisibility, or screen filter.
    pub effect_type: u32,
    /// Four uninterpreted filter words; their interpretation depends on type.
    pub parameters: [u32; 4],
    /// Native 7ECEC0 rejects conditions outside zero through seven.
    pub light_condition: Option<WorldLightCondition>,
    /// Authored references passed to the two zone-sound override owners.
    pub sound_references: [u32; 2],
}

/// Resident declarations; an absent row clears the current override.
#[derive(Default)]
pub struct ScreenEffectCatalog {
    definitions: HashMap<u32, ScreenEffectDefinition>,
}

impl ScreenEffectCatalog {
    /// Loads the exact ten-word build-12340 schema.
    ///
    /// # Errors
    /// Rejects missing data, wrong layouts, truncated rows and duplicate IDs.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient/ScreenEffect.dbc")?)?;
        if table.header().field_count() != 10 || table.header().record_size() != 40 {
            return Err(database_error(
                &table,
                "expected build-12340 ScreenEffect.dbc with ten four-byte fields".into(),
            ));
        }
        let mut definitions = HashMap::new();
        for row in 0..table.header().record_count() {
            let word = |column| {
                table.field_u32(row, column).ok_or_else(|| {
                    database_error(
                        &table,
                        format!("missing screen-effect row {row} column {column}"),
                    )
                })
            };
            let id = word(0)?;
            let definition = ScreenEffectDefinition {
                effect_type: word(2)?,
                parameters: [word(3)?, word(4)?, word(5)?, word(6)?],
                light_condition: u8::try_from(word(7)?)
                    .ok()
                    .and_then(WorldLightCondition::new),
                sound_references: [word(8)?, word(9)?],
            };
            if definitions.insert(id, definition).is_some() {
                return Err(database_error(
                    &table,
                    format!("duplicate screen-effect ID {id}"),
                ));
            }
        }
        Ok(Self { definitions })
    }

    /// Returns the authored declaration only when its row exists.
    #[must_use]
    pub fn definition(&self, id: u32) -> Option<ScreenEffectDefinition> {
        self.definitions.get(&id).copied()
    }
}
