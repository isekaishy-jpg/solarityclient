//! Exact build-12340 `SoundEmitters.dbc` rows.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::localized::database_error;
use super::super::wow_client_db::WdbcTable;

const SOUND_EMITTERS_PATH: &str = "DBFilesClient\\SoundEmitters.dbc";
const SOUND_EMITTERS_FIELD_COUNT: u32 = 10;

/// One globally authored sound-emitter row.
///
/// Build 12340 loads this table, but the executable has no playback references
/// to its indexed records. It therefore remains data rather than being merged
/// into the live MCSE or liquid sound services.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEmitterDefinition {
    id: u32,
    position: [f32; 3],
    direction: [f32; 3],
    advanced_sound_entry_id: u32,
    map_id: u32,
    name: String,
}

impl SoundEmitterDefinition {
    /// Returns the table's primary key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored world position.
    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns the authored emitter direction.
    #[must_use]
    pub const fn direction(&self) -> [f32; 3] {
        self.direction
    }

    /// Returns the `SoundEntriesAdvanced.dbc` relation.
    #[must_use]
    pub const fn advanced_sound_entry_id(&self) -> u32 {
        self.advanced_sound_entry_id
    }

    /// Returns the authored `Map.dbc` relation.
    #[must_use]
    pub const fn map_id(&self) -> u32 {
        self.map_id
    }

    /// Returns the row's diagnostic name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Identifier-indexed global sound-emitter data.
pub struct SoundEmitterCatalog {
    entries: Vec<SoundEmitterDefinition>,
}

impl SoundEmitterCatalog {
    /// Loads the exact ten-word build-12340 layout through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, schema, string, finite-float, or
    /// duplicate-primary-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(SOUND_EMITTERS_PATH)?)?;
        require_layout(&table)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            entries.push(SoundEmitterDefinition {
                id: field(&table, row, 0)?,
                position: finite_float_array(&table, row, 1)?,
                direction: finite_float_array(&table, row, 4)?,
                advanced_sound_entry_id: field(&table, row, 7)?,
                map_id: field(&table, row, 8)?,
                name: string(&table, row, 9)?,
            });
        }
        entries.sort_unstable_by_key(SoundEmitterDefinition::id);
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds one exact emitter identifier.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&SoundEmitterDefinition> {
        self.entries
            .binary_search_by_key(&id, SoundEmitterDefinition::id)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Returns every row in ascending identifier order.
    #[must_use]
    pub fn entries(&self) -> &[SoundEmitterDefinition] {
        &self.entries
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == SOUND_EMITTERS_FIELD_COUNT
        && header.record_size() == SOUND_EMITTERS_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 SoundEmitters.dbc requires 10 fields and 40-byte records; found {} fields and {}-byte records",
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

fn finite_float_array<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first_column: u32,
) -> Result<[f32; N], AssetError> {
    let mut values = [0.0; N];
    for (index, value) in values.iter_mut().enumerate() {
        let column = first_column + index as u32;
        *value = f32::from_bits(field(table, row, column)?);
        if !value.is_finite() {
            return Err(database_error(
                table,
                format!("record {row} field {column} is not finite"),
            ));
        }
    }
    Ok(values)
}

fn string(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|source| database_error(table, source.to_string()))
}
