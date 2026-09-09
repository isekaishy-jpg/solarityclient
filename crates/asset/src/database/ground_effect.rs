//! Stock terrain-detail model identities and weighted texture definitions.

use std::collections::BTreeMap;

use crate::{AssetError, AssetPath, AssetStore};

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

/// One `GroundEffectDoodad.dbc` model consumed by native 7B3050 and 7B1B50.
pub struct GroundEffectDoodad {
    id: u32,
    path: AssetPath,
    flags: u32,
}

impl GroundEffectDoodad {
    /// Returns the ground-effect model identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the model below stock's `World/NoDXT/Detail` directory.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns authored flags: bit zero aligns to slope; bit one ignores MCCV.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// One `GroundEffectTexture.dbc` distribution selected by an MCNK layer.
#[derive(Clone, Copy, Debug)]
pub struct GroundEffectTexture {
    models: [u32; 4],
    weights: [u32; 4],
    density: u32,
}

impl GroundEffectTexture {
    /// Returns the four authored doodad identifiers, including empty slots.
    #[must_use]
    pub const fn models(self) -> [u32; 4] {
        self.models
    }

    /// Returns each slot's weight in the native sixteen-entry distribution.
    #[must_use]
    pub const fn weights(self) -> [u32; 4] {
        self.weights
    }

    /// Returns placements per selected cell; native 7D3390 maps zero to eight.
    #[must_use]
    pub const fn density(self) -> u32 {
        if self.density == 0 { 8 } else { self.density }
    }
}

/// The two stock tables needed to generate and resolve terrain detail.
pub struct GroundEffectCatalog {
    doodads: BTreeMap<u32, GroundEffectDoodad>,
    textures: BTreeMap<u32, GroundEffectTexture>,
}

impl GroundEffectCatalog {
    /// Loads the exact three-word doodad and eleven-word texture schemas.
    ///
    /// # Errors
    /// Returns [`AssetError`] for storage, schema, string, or duplicate keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = load_table(store, "GroundEffectDoodad", 3)?;
        let mut doodads = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let id = field(&table, row, 0)?;
            let name = table.string_bytes(field(&table, row, 1)?).ok_or_else(|| {
                database_error(&table, format!("record {row} has an invalid model string"))
            })?;
            let name = std::str::from_utf8(name)
                .map_err(|error| database_error(&table, error.to_string()))?;
            let entry = GroundEffectDoodad {
                id,
                path: AssetPath::new(format!("World\\NoDXT\\Detail\\{name}"))?,
                flags: field(&table, row, 2)?,
            };
            insert(&mut doodads, id, entry, &table)?;
        }
        let table = load_table(store, "GroundEffectTexture", 11)?;
        let mut textures = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let mut models = [0; 4];
            let mut weights = [0; 4];
            for index in 0..4 {
                models[index] = field(&table, row, index as u32 + 1)?;
                weights[index] = field(&table, row, index as u32 + 5)?;
            }
            insert(
                &mut textures,
                field(&table, row, 0)?,
                GroundEffectTexture {
                    models,
                    weights,
                    density: field(&table, row, 9)?,
                },
                &table,
            )?;
        }
        Ok(Self { doodads, textures })
    }

    /// Finds the exact model referenced by a distribution slot.
    #[must_use]
    pub fn doodad(&self, id: u32) -> Option<&GroundEffectDoodad> {
        self.doodads.get(&id)
    }

    /// Finds the exact distribution referenced by an MCNK texture layer.
    #[must_use]
    pub fn texture(&self, id: u32) -> Option<GroundEffectTexture> {
        self.textures.get(&id).copied()
    }
}

/// Validates the pinned table layout before accessing row fields.
fn load_table(store: &mut AssetStore, name: &str, fields: u32) -> Result<WdbcTable, AssetError> {
    let table = WdbcTable::load(
        store,
        &AssetPath::new(format!("DBFilesClient\\{name}.dbc"))?,
    )?;
    if table.header().field_count() != fields || table.header().record_size() != fields * 4 {
        return Err(database_error(
            &table,
            format!(
                "build-12340 {name}.dbc requires {fields} fields and {}-byte records",
                fields * 4
            ),
        ));
    }
    Ok(table)
}

/// Reports malformed fields with the owning table and row.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Preserves the database boundary's duplicate-key rejection policy.
fn insert<T>(
    entries: &mut BTreeMap<u32, T>,
    id: u32,
    value: T,
    table: &WdbcTable,
) -> Result<(), AssetError> {
    if entries.insert(id, value).is_some() {
        return Err(database_error(table, format!("duplicate primary key {id}")));
    }
    Ok(())
}
