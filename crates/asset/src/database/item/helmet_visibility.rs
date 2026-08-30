//! Exact build-12340 helmet geoset-visibility masks.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const HELMET_GEOSET_VISIBILITY_PATH: &str = "DBFilesClient\\HelmetGeosetVisData.dbc";
const HELMET_GEOSET_VISIBILITY_FIELD_COUNT: u32 = 8;

/// One helmet row controlling character hair and facial geoset visibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelmetGeosetVisibility {
    id: u32,
    hide_flags: [u32; 7],
}

impl HelmetGeosetVisibility {
    /// Returns the identifier referenced by `ItemDisplayInfo.dbc`.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the hair geoset visibility flags.
    #[must_use]
    pub const fn hair_flags(self) -> u32 {
        self.hide_flags[0]
    }

    /// Returns the three facial-feature visibility flag words.
    #[must_use]
    pub const fn facial_hair_flags(self) -> [u32; 3] {
        [self.hide_flags[1], self.hide_flags[2], self.hide_flags[3]]
    }

    /// Returns the ear geoset visibility flags.
    #[must_use]
    pub const fn ear_flags(self) -> u32 {
        self.hide_flags[4]
    }

    /// Preserves the two build-12340 words whose stock meaning is unrecovered.
    #[must_use]
    pub const fn additional_flags(self) -> [u32; 2] {
        [self.hide_flags[5], self.hide_flags[6]]
    }

    /// Returns all seven authored visibility words in physical table order.
    #[must_use]
    pub const fn hide_flags(self) -> [u32; 7] {
        self.hide_flags
    }
}

/// Identifier-indexed build-12340 helmet visibility records.
pub struct HelmetGeosetVisibilityCatalog {
    rows: Vec<HelmetGeosetVisibility>,
}

impl HelmetGeosetVisibilityCatalog {
    /// Loads the exact eight-field table through stock archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(HELMET_GEOSET_VISIBILITY_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut rows = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let fields = read_fields(&table, row)?;
            rows.push(HelmetGeosetVisibility {
                id: fields[0],
                hide_flags: fields[1..8].try_into().map_err(|_| {
                    database_error(
                        &table,
                        format!("record {row} visibility words are truncated"),
                    )
                })?,
            });
        }
        rows.sort_unstable_by_key(|row| row.id);
        if let Some(duplicate) = rows.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { rows })
    }

    /// Finds one exact visibility identifier without substituting another row.
    #[must_use]
    pub fn visibility(&self, id: u32) -> Option<&HelmetGeosetVisibility> {
        self.rows
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|index| &self.rows[index])
    }
}

/// Rejects another build's row width before interpreting flag positions.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == HELMET_GEOSET_VISIBILITY_FIELD_COUNT
        && header.record_size() == HELMET_GEOSET_VISIBILITY_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 HelmetGeosetVisData.dbc requires 8 fields and 32-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one exact visibility row through checked table access.
fn read_fields(table: &WdbcTable, row: u32) -> Result<[u32; 8], AssetError> {
    let mut fields = [0_u32; 8];
    for (field, value) in fields.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(fields)
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
