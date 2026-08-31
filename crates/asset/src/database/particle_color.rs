//! Exact build-12340 `ParticleColor.dbc` replacement ramps.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const PARTICLE_COLOR_PATH: &str = "DBFilesClient\\ParticleColor.dbc";
const PARTICLE_COLOR_FIELD_COUNT: u32 = 10;

/// One three-stage replacement ramp selected by creature or item presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParticleColorDefinition {
    id: u32,
    start: [u32; 3],
    middle: [u32; 3],
    end: [u32; 3],
}

impl ParticleColorDefinition {
    /// Returns the identifier referenced by display metadata.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the three packed start colors in stock order.
    #[must_use]
    pub const fn start(self) -> [u32; 3] {
        self.start
    }

    /// Returns the three packed midpoint colors in stock order.
    #[must_use]
    pub const fn middle(self) -> [u32; 3] {
        self.middle
    }

    /// Returns the three packed end colors in stock order.
    #[must_use]
    pub const fn end(self) -> [u32; 3] {
        self.end
    }
}

/// Identifier-indexed particle replacement colors shared by all M2 assets.
pub struct ParticleColorCatalog {
    definitions: Vec<ParticleColorDefinition>,
}

impl ParticleColorCatalog {
    /// Loads the exact ten-word build-12340 table through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, uses
    /// another layout, truncates a row, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(PARTICLE_COLOR_PATH)?)?;
        require_layout(&table)?;
        let mut definitions = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let fields = read_fields(&table, row)?;
            definitions.push(ParticleColorDefinition {
                id: fields[0],
                start: [fields[1], fields[2], fields[3]],
                middle: [fields[4], fields[5], fields[6]],
                end: [fields[7], fields[8], fields[9]],
            });
        }
        definitions.sort_unstable_by_key(|definition| definition.id);
        if let Some(duplicate) = definitions.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { definitions })
    }

    /// Finds one exact replacement identifier without substituting another row.
    #[must_use]
    pub fn definition(&self, id: u32) -> Option<&ParticleColorDefinition> {
        self.definitions
            .binary_search_by_key(&id, |definition| definition.id)
            .ok()
            .map(|index| &self.definitions[index])
    }
}

/// Rejects another client layout before interpreting row fields.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == PARTICLE_COLOR_FIELD_COUNT
        && header.record_size() == PARTICLE_COLOR_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 ParticleColor.dbc requires 10 fields and 40-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one exact fixed-width row through checked WDBC access.
fn read_fields(table: &WdbcTable, row: u32) -> Result<[u32; 10], AssetError> {
    let mut fields = [0_u32; 10];
    for (field, value) in fields.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(fields)
}
