//! Exact build-12340 material dispatch and liquid queue flags.

use crate::{AssetError, AssetPath, AssetStore};

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

/// One `LiquidMaterial.dbc` row consumed by native 8A27C0 and 79B870.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiquidMaterialDefinition {
    id: u32,
    shader: u32,
    flags: u32,
}

impl LiquidMaterialDefinition {
    /// Returns the key referenced by `LiquidType.dbc`.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the authored shader family: zero for water, one for magma.
    #[must_use]
    pub const fn shader(self) -> u32 {
        self.shader
    }

    /// Returns the complete material flags; bit zero selects the late queue.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Selects native 8A20C0's queue after the opaque terrain/WMO pass.
    #[must_use]
    pub const fn is_transparent(self) -> bool {
        self.flags & 1 != 0
    }
}

/// Identifier-indexed material dispatch, separate from liquid type definitions.
pub struct LiquidMaterialCatalog {
    entries: Vec<LiquidMaterialDefinition>,
}

impl LiquidMaterialCatalog {
    /// Loads the exact three-word schema through the selected archive stack.
    ///
    /// # Errors
    /// Returns [`AssetError`] for storage, schema, or duplicate-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient\\LiquidMaterial.dbc")?)?;
        let header = table.header();
        if header.field_count() != 3 || header.record_size() != 12 {
            return Err(database_error(
                &table,
                format!(
                    "build-12340 LiquidMaterial.dbc requires 3 fields and 12-byte records; found {} fields and {}-byte records",
                    header.field_count(),
                    header.record_size()
                ),
            ));
        }
        let mut entries = Vec::with_capacity(header.record_count() as usize);
        for row in 0..header.record_count() {
            let field = |column| {
                table.field_u32(row, column).ok_or_else(|| {
                    database_error(&table, format!("record {row} field {column} is truncated"))
                })
            };
            entries.push(LiquidMaterialDefinition {
                id: field(0)?,
                shader: field(1)?,
                flags: field(2)?,
            });
        }
        entries.sort_unstable_by_key(|entry| entry.id);
        if let Some(pair) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", pair[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds one exact material identifier without substituting another row.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<LiquidMaterialDefinition> {
        self.entries
            .binary_search_by_key(&id, |entry| entry.id)
            .ok()
            .map(|index| self.entries[index])
    }
}
