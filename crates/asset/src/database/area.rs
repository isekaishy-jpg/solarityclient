//! Localized AreaTable identities and inherited world environment relations.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::{database_error, localized_string};
use super::sound_environment::{AreaSoundReferences, LiquidTypeCatalog, LiquidTypeDefinition};
use super::wow_client_db::WdbcTable;

const AREA_TABLE_PATH: &str = "DBFilesClient\\AreaTable.dbc";
const AREA_TABLE_FIELD_COUNT: u32 = 36;
const LOCALIZED_NAME_FIRST_FIELD: u32 = 11;

/// One build-12340 area identity with world sound and liquid relations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaDefinition {
    id: u32,
    parent_area_id: u32,
    name: String,
    sounds: AreaSoundReferences,
    /// Zero retains the original liquid or consults the area's immediate parent.
    liquid_overrides: [u32; 4],
}

impl AreaDefinition {
    /// Returns independently inherited zone sound and provider relations.
    pub const fn sounds(&self) -> AreaSoundReferences {
        self.sounds
    }
    /// Returns the AreaTable identifier carried in character enumeration.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the containing zone identifier, or zero for a top-level zone.
    #[must_use]
    pub const fn parent_area_id(&self) -> u32 {
        self.parent_area_id
    }

    /// Returns the exact selected-locale area name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Identifier-indexed build-12340 area names and world environment relations.
pub struct AreaTableCatalog {
    areas: Vec<AreaDefinition>,
}

impl AreaTableCatalog {
    /// Loads the exact 36-word AreaTable layout.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, layout, text, or duplicate-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let path = AssetPath::new(AREA_TABLE_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;
        let mut areas = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            areas.push(AreaDefinition {
                id: field(&table, row, 0)?,
                parent_area_id: field(&table, row, 2)?,
                name: localized_string(&table, row, LOCALIZED_NAME_FIRST_FIELD, locale)?,
                sounds: AreaSoundReferences::read(&table, row, 5)?,
                liquid_overrides: [
                    field(&table, row, 29)?,
                    field(&table, row, 30)?,
                    field(&table, row, 31)?,
                    field(&table, row, 32)?,
                ],
            });
        }
        areas.sort_unstable_by_key(AreaDefinition::id);
        if let Some(duplicate) = areas.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { areas })
    }

    /// Finds one exact area identifier.
    #[must_use]
    pub fn area(&self, id: u32) -> Option<&AreaDefinition> {
        self.areas
            .binary_search_by_key(&id, AreaDefinition::id)
            .ok()
            .map(|index| &self.areas[index])
    }

    /// Resolves 9905C0's single-parent liquid substitution for wet footsteps,
    /// splashes, and ripple admission. Only liquid IDs 1..20 use area overrides;
    /// an absent area does not inherit and zero replacements retain the input.
    #[must_use]
    pub fn liquid_flags(&self, liquids: &LiquidTypeCatalog, area: u32, liquid: u32) -> Option<u32> {
        if liquid == 0 {
            return None;
        }
        let mut resolved = liquid;
        if liquid < 21
            && let Some(area) = self.area(area)
        {
            let slot = ((liquid - 1) & 3) as usize;
            let replacement = if area.liquid_overrides[slot] == 0 && area.parent_area_id != 0 {
                self.area(area.parent_area_id)
                    .map_or(0, |parent| parent.liquid_overrides[slot])
            } else {
                area.liquid_overrides[slot]
            };
            if replacement != 0 {
                resolved = replacement;
            }
        }
        liquids.entry(resolved).map(LiquidTypeDefinition::flags)
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == AREA_TABLE_FIELD_COUNT
        && header.record_size() == AREA_TABLE_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 AreaTable.dbc requires 36 fields and 144-byte records; found {} fields and {}-byte records",
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
