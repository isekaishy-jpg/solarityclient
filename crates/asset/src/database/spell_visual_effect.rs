//! Build-12340 `SpellVisualEffectName.dbc`, including the CEffect name bank.

use std::collections::BTreeMap;

use crate::{AssetError, AssetPath, AssetStore};

use super::{WdbcTable, localized::database_error};

/// An authored effect model and its independent area and scale parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellVisualEffectDefinition {
    id: u32,
    name: String,
    model_name: String,
    area_effect_size: f32,
    scale: f32,
    min_scale: f32,
    max_scale: f32,
}

impl SpellVisualEffectDefinition {
    /// Returns the primary key used by spell visuals and CEffect.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the exact, case-sensitive name used by CEffect initialization.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the authored model name, including unused legacy export paths.
    #[must_use]
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Resolves an archive-relative model path when this effect is requested.
    ///
    /// Stock contains unused absolute developer export paths. Those rows must
    /// not prevent loading other effects, and must never trigger filesystem
    /// or network reads. Empty names and bare directories mean no model.
    ///
    /// # Errors
    ///
    /// Returns an asset-path error for a non-archive-relative model name.
    pub fn model_path(&self) -> Result<Option<AssetPath>, AssetError> {
        if self.model_name.is_empty() || self.model_name.ends_with(['\\', '/']) {
            Ok(None)
        } else {
            AssetPath::new(&self.model_name).map(Some)
        }
    }

    /// Returns the area-effect size, separate from model scale.
    #[must_use]
    pub const fn area_effect_size(&self) -> f32 {
        self.area_effect_size
    }

    /// Returns the authored model scale multiplier.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns the minimum final scale used by CEffect placement.
    #[must_use]
    pub const fn min_scale(&self) -> f32 {
        self.min_scale
    }

    /// Returns the maximum final scale used by CEffect placement.
    #[must_use]
    pub const fn max_scale(&self) -> f32 {
        self.max_scale
    }
}

/// Authored effects resolved by identifier or native initialization name.
pub struct SpellVisualEffectCatalog {
    definitions: Vec<SpellVisualEffectDefinition>,
    names: BTreeMap<String, u32>,
}

impl SpellVisualEffectCatalog {
    /// Loads the seven-word table through normal archive precedence.
    ///
    /// Names match exactly. When several rows have the same name, the last
    /// physical row wins, as in build 12340's initialization at `0x006F7520`.
    ///
    /// # Errors
    ///
    /// Returns an asset error for missing or malformed data, a different
    /// schema, invalid strings, or duplicate primary keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(
            store,
            &AssetPath::new("DBFilesClient/SpellVisualEffectName.dbc")?,
        )?;
        if table.header().field_count() != 7 || table.header().record_size() != 28 {
            return Err(database_error(
                &table,
                "build-12340 SpellVisualEffectName.dbc requires 7 fields and 28-byte records"
                    .to_owned(),
            ));
        }
        let mut definitions = Vec::with_capacity(table.header().record_count() as usize);
        let mut names = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let mut fields = [0; 7];
            for (column, value) in fields.iter_mut().enumerate() {
                *value = table.field_u32(row, column as u32).ok_or_else(|| {
                    database_error(&table, format!("record {row} field {column} is truncated"))
                })?;
            }
            let name = string(&table, row, fields[1])?;
            let model_name = string(&table, row, fields[2])?;
            names.insert(name.clone(), fields[0]);
            definitions.push(SpellVisualEffectDefinition {
                id: fields[0],
                name,
                model_name,
                area_effect_size: f32::from_bits(fields[3]),
                scale: f32::from_bits(fields[4]),
                min_scale: f32::from_bits(fields[5]),
                max_scale: f32::from_bits(fields[6]),
            });
        }
        definitions.sort_unstable_by_key(|definition| definition.id);
        if let Some(pair) = definitions.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", pair[0].id),
            ));
        }
        Ok(Self { definitions, names })
    }

    /// Resolves an exact identifier without substituting another effect.
    #[must_use]
    pub fn definition(&self, id: u32) -> Option<&SpellVisualEffectDefinition> {
        self.definitions
            .binary_search_by_key(&id, |definition| definition.id)
            .ok()
            .map(|index| &self.definitions[index])
    }

    /// Resolves an exact name using the last matching physical table row.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&SpellVisualEffectDefinition> {
        self.definition(*self.names.get(name)?)
    }
}

fn string(table: &WdbcTable, row: u32, offset: u32) -> Result<String, AssetError> {
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec())
        .map_err(|error| database_error(table, format!("record {row}: {error}")))
}
