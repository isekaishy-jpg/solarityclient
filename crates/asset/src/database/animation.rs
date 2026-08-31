//! Exact build-12340 animation behavior and fallback metadata.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const ANIMATION_DATA_PATH: &str = "DBFilesClient\\AnimationData.dbc";
const ANIMATION_DATA_FIELD_COUNT: u32 = 8;

/// One row used by stock to map gameplay behaviors onto model sequences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationDataDefinition {
    id: u32,
    name: String,
    weapon_flags: u32,
    body_flags: u32,
    flags: u32,
    fallback_id: u32,
    behavior_id: u32,
    behavior_tier: u32,
}

impl AnimationDataDefinition {
    /// Returns the identifier stored in M2 animation-sequence records.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the stable diagnostic name authored by the client table.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the complete weapon-presentation flag word.
    #[must_use]
    pub const fn weapon_flags(&self) -> u32 {
        self.weapon_flags
    }

    /// Returns the complete body-presentation flag word.
    #[must_use]
    pub const fn body_flags(&self) -> u32 {
        self.body_flags
    }

    /// Returns the complete animation behavior flag word.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the next behavior tried when an M2 omits this sequence.
    #[must_use]
    pub const fn fallback_id(&self) -> u32 {
        self.fallback_id
    }

    /// Returns the base behavior shared by tier-specific rows.
    #[must_use]
    pub const fn behavior_id(&self) -> u32 {
        self.behavior_id
    }

    /// Returns the numeric unit animation tier selecting this row.
    #[must_use]
    pub const fn behavior_tier(&self) -> u32 {
        self.behavior_tier
    }
}

/// Identifier-indexed animation metadata used before selecting an M2 sequence.
pub struct AnimationDataCatalog {
    definitions: Vec<AnimationDataDefinition>,
}

impl AnimationDataCatalog {
    /// Loads the exact eight-word table through ordinary archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, uses a
    /// different layout, contains invalid UTF-8 text, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(ANIMATION_DATA_PATH)?)?;
        require_layout(&table)?;
        let mut definitions = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let fields = read_fields(&table, row)?;
            definitions.push(AnimationDataDefinition {
                id: fields[0],
                name: string(&table, row, fields[1])?,
                weapon_flags: fields[2],
                body_flags: fields[3],
                flags: fields[4],
                fallback_id: fields[5],
                behavior_id: fields[6],
                behavior_tier: fields[7],
            });
        }
        definitions.sort_unstable_by_key(AnimationDataDefinition::id);
        if let Some(duplicate) = definitions.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { definitions })
    }

    /// Finds one exact animation identifier without substituting another row.
    #[must_use]
    pub fn definition(&self, id: u32) -> Option<&AnimationDataDefinition> {
        self.definitions
            .binary_search_by_key(&id, AnimationDataDefinition::id)
            .ok()
            .map(|index| &self.definitions[index])
    }

    /// Finds the row implementing one base behavior at a specific unit tier.
    ///
    /// A nonzero-tier identifier is already a resolved row in stock and is
    /// returned unchanged. Ground-tier base rows similarly resolve directly.
    #[must_use]
    pub fn tiered_definition(
        &self,
        behavior_id: u32,
        behavior_tier: u32,
    ) -> Option<&AnimationDataDefinition> {
        if let Some(direct) = self.definition(behavior_id)
            && (direct.behavior_tier != 0
                || (behavior_tier == 0
                    && direct.behavior_id == behavior_id
                    && direct.behavior_tier == 0))
        {
            return Some(direct);
        }
        self.definitions.iter().find(|definition| {
            definition.behavior_id == behavior_id && definition.behavior_tier == behavior_tier
        })
    }

    /// Returns every row in ascending identifier order.
    #[must_use]
    pub fn definitions(&self) -> &[AnimationDataDefinition] {
        &self.definitions
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == ANIMATION_DATA_FIELD_COUNT
        && header.record_size() == ANIMATION_DATA_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 AnimationData.dbc requires 8 fields and 32-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

fn read_fields(table: &WdbcTable, row: u32) -> Result<[u32; 8], AssetError> {
    let mut fields = [0_u32; 8];
    for (field, value) in fields.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(fields)
}

fn string(table: &WdbcTable, row: u32, offset: u32) -> Result<String, AssetError> {
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field 1 has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}
