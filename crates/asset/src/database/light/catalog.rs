//! Exact WDBC loading and identifier indexes for outdoor environments.

use std::collections::HashMap;

use glam::Vec3;

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::types::{BAND_KEY_COUNT, LightBand, LightDefinition, LightParameter, LightSkybox};
use super::{ModelLightColors, WorldLightQuery, WorldLightSample, WorldLightSampleError};
use crate::database::wow_client_db::WdbcTable;

const LIGHT_PATH: &str = "DBFilesClient\\Light.dbc";
const LIGHT_PARAMETER_PATH: &str = "DBFilesClient\\LightParams.dbc";
const LIGHT_SKYBOX_PATH: &str = "DBFilesClient\\LightSkybox.dbc";
const LIGHT_COLOR_BAND_PATH: &str = "DBFilesClient\\LightIntBand.dbc";
const LIGHT_FLOAT_BAND_PATH: &str = "DBFilesClient\\LightFloatBand.dbc";

const CLIENT_COORDINATE_SCALE: f32 = 1.0 / 36.0;
const CLIENT_MAP_ORIGIN: f32 = 17_066.666;

/// Loaded build-12340 exterior light definitions and every sampled channel.
pub struct LightCatalog {
    pub(super) lights: Vec<LightDefinition>,
    pub(super) parameters: HashMap<u32, LightParameter>,
    skyboxes: HashMap<u32, LightSkybox>,
    pub(super) color_bands: HashMap<u32, LightBand<u32>>,
    pub(super) float_bands: HashMap<u32, LightBand<f32>>,
}

impl LightCatalog {
    /// Loads the exact five-table WotLK environment schema.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for archive resolution, exact-layout, invalid
    /// string/float, band-key, or duplicate-primary-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let light_table = load_table(store, LIGHT_PATH)?;
        let parameter_table = load_table(store, LIGHT_PARAMETER_PATH)?;
        let skybox_table = load_table(store, LIGHT_SKYBOX_PATH)?;
        let color_table = load_table(store, LIGHT_COLOR_BAND_PATH)?;
        let float_table = load_table(store, LIGHT_FLOAT_BAND_PATH)?;
        require_layout(&light_table, 15, "Light.dbc")?;
        require_layout(&parameter_table, 9, "LightParams.dbc")?;
        require_layout(&skybox_table, 3, "LightSkybox.dbc")?;
        require_layout(&color_table, 34, "LightIntBand.dbc")?;
        require_layout(&float_table, 34, "LightFloatBand.dbc")?;

        Ok(Self {
            lights: decode_lights(&light_table)?,
            parameters: decode_parameters(&parameter_table)?,
            skyboxes: decode_skyboxes(&skybox_table)?,
            color_bands: decode_color_bands(&color_table)?,
            float_bands: decode_float_bands(&float_table)?,
        })
    }

    /// Returns Light.dbc rows in authored table order.
    #[must_use]
    pub fn lights(&self) -> &[LightDefinition] {
        &self.lights
    }

    /// Finds one exact LightParams.dbc identifier.
    #[must_use]
    pub fn parameter(&self, id: u32) -> Option<&LightParameter> {
        self.parameters.get(&id)
    }

    /// Finds one exact LightSkybox.dbc identifier.
    #[must_use]
    pub fn skybox(&self, id: u32) -> Option<&LightSkybox> {
        self.skyboxes.get(&id)
    }

    /// Joins and blends the complete stock exterior environment.
    ///
    /// # Errors
    ///
    /// Returns [`WorldLightSampleError`] when the query or required authored
    /// row/band is absent. No generic light or nearest-volume substitute is used.
    pub fn sample(
        &self,
        query: WorldLightQuery,
    ) -> Result<WorldLightSample, WorldLightSampleError> {
        super::sampling::sample(self, query)
    }

    /// Samples a complete LightParams override, as LiquidType.LightID does
    /// at `0x007F32EA`. The identifier belongs to LightParams, not Light.dbc.
    ///
    /// # Errors
    /// Returns an error for an absent parameter or any required band.
    pub fn sample_parameter(
        &self,
        parameter_id: u32,
        half_minutes: u32,
    ) -> Result<WorldLightSample, WorldLightSampleError> {
        super::sampling::sample_parameter(self, parameter_id, half_minutes)
    }

    /// Samples an M2 palette directly from LightParams and its first two bands.
    ///
    /// Build 12340's Glue ghost callback at `0x004E3A20` uses parameter three
    /// at time zero through `0x007EBF30`, without a world-volume lookup.
    ///
    /// # Errors
    ///
    /// Returns [`WorldLightSampleError`] for an absent parameter or required
    /// color band. Unrelated sky and scalar channels are not required.
    pub fn model_light_colors(
        &self,
        parameter_id: u32,
        half_minutes: u32,
    ) -> Result<ModelLightColors, WorldLightSampleError> {
        super::sampling::model_light_colors(self, parameter_id, half_minutes)
    }
}

/// Loads one table through the ordinary archive-selected arbitrary-file path.
fn load_table(store: &mut AssetStore, path: &str) -> Result<WdbcTable, AssetError> {
    WdbcTable::load(store, &AssetPath::new(path)?)
}

/// Enforces the exact word-aligned build-12340 schema.
fn require_layout(table: &WdbcTable, fields: u32, name: &str) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == fields && header.record_size() == fields * 4 {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 {name} requires {fields} fields and {}-byte records; found {} fields and {}-byte records",
            fields * 4,
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Decodes Light.dbc's coordinate transformation and parameter slots.
fn decode_lights(table: &WdbcTable) -> Result<Vec<LightDefinition>, AssetError> {
    let mut lights = Vec::with_capacity(table.header().record_count() as usize);
    let mut ids = HashMap::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let id = field(table, row, 0)?;
        reject_duplicate(table, &mut ids, id, row)?;
        let raw = Vec3::new(
            float_field(table, row, 2)?,
            float_field(table, row, 3)?,
            float_field(table, row, 4)?,
        );
        let is_global = raw == Vec3::ZERO;
        let dbc = raw * CLIENT_COORDINATE_SCALE;
        let position = Vec3::new(CLIENT_MAP_ORIGIN - dbc.z, CLIENT_MAP_ORIGIN - dbc.x, dbc.y);
        let falloff_start = float_field(table, row, 5)? * CLIENT_COORDINATE_SCALE;
        let falloff_end = float_field(table, row, 6)? * CLIENT_COORDINATE_SCALE;
        if falloff_start < 0.0 || falloff_end < falloff_start {
            return Err(database_error(
                table,
                format!("record {row} has invalid light falloff radii"),
            ));
        }
        let mut parameter_ids = [0; 8];
        for (index, parameter_id) in parameter_ids.iter_mut().enumerate() {
            *parameter_id = field(table, row, 7 + index as u32)?;
        }
        lights.push(LightDefinition {
            id,
            map_id: field(table, row, 1)?,
            is_global,
            position,
            falloff_start,
            falloff_end,
            parameter_ids,
        });
    }
    Ok(lights)
}

/// Decodes the pre-Cataclysm nine-field LightParams layout.
fn decode_parameters(table: &WdbcTable) -> Result<HashMap<u32, LightParameter>, AssetError> {
    let mut parameters = HashMap::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let id = field(table, row, 0)?;
        if id == 0
            || id
                .checked_mul(18)
                .and_then(|value| value.checked_sub(17))
                .is_none()
        {
            return Err(database_error(
                table,
                format!("record {row} has invalid light parameter ID {id}"),
            ));
        }
        let parameter = LightParameter {
            id,
            highlight_sky: field(table, row, 1)?,
            skybox_id: field(table, row, 2)?,
            // Native 7EC1D0..7EC20D reads glow at +10, then four alphas.
            // 8A2BF0 consumes the first pair for ocean and the second for river.
            glow: float_field(table, row, 4)?,
            river_shallow_alpha: float_field(table, row, 7)?,
            river_deep_alpha: float_field(table, row, 8)?,
            ocean_shallow_alpha: float_field(table, row, 5)?,
            ocean_deep_alpha: float_field(table, row, 6)?,
        };
        if parameters.insert(id, parameter).is_some() {
            return Err(database_error(
                table,
                format!("duplicate primary key {id} at record {row}"),
            ));
        }
    }
    Ok(parameters)
}

/// Decodes model paths and flags from exact 12-byte skybox rows.
fn decode_skyboxes(table: &WdbcTable) -> Result<HashMap<u32, LightSkybox>, AssetError> {
    let mut skyboxes = HashMap::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let id = field(table, row, 0)?;
        let offset = field(table, row, 1)?;
        let bytes = table.string_bytes(offset).ok_or_else(|| {
            database_error(
                table,
                format!("record {row} model path offset {offset} is invalid"),
            )
        })?;
        let model_path = std::str::from_utf8(bytes)
            .map_err(|source| database_error(table, format!("record {row} model path: {source}")))?
            .to_owned();
        let skybox = LightSkybox {
            id,
            model_path,
            flags: field(table, row, 2)?,
        };
        if skyboxes.insert(id, skybox).is_some() {
            return Err(database_error(
                table,
                format!("duplicate primary key {id} at record {row}"),
            ));
        }
    }
    Ok(skyboxes)
}

/// Decodes packed RGB keys without applying color-space conversion.
fn decode_color_bands(table: &WdbcTable) -> Result<HashMap<u32, LightBand<u32>>, AssetError> {
    let mut bands = HashMap::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let id = field(table, row, 0)?;
        let entries = band_entries(table, row)?;
        let band = LightBand {
            entries,
            times: band_times(table, row)?,
            values: read_fields(table, row, 18)?,
        };
        if bands.insert(id, band).is_some() {
            return Err(database_error(
                table,
                format!("duplicate primary key {id} at record {row}"),
            ));
        }
    }
    Ok(bands)
}

/// Decodes the six scalar channel families and rejects non-finite keys.
fn decode_float_bands(table: &WdbcTable) -> Result<HashMap<u32, LightBand<f32>>, AssetError> {
    let mut bands = HashMap::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let id = field(table, row, 0)?;
        let entries = band_entries(table, row)?;
        let words = read_fields::<BAND_KEY_COUNT>(table, row, 18)?;
        let mut values = [0.0; BAND_KEY_COUNT];
        for (index, word) in words.into_iter().enumerate() {
            let value = f32::from_bits(word);
            if !value.is_finite() {
                return Err(database_error(
                    table,
                    format!("record {row} float key {index} is not finite"),
                ));
            }
            values[index] = value;
        }
        let band = LightBand {
            entries,
            times: band_times(table, row)?,
            values,
        };
        if bands.insert(id, band).is_some() {
            return Err(database_error(
                table,
                format!("duplicate primary key {id} at record {row}"),
            ));
        }
    }
    Ok(bands)
}

/// Validates the closed zero-through-sixteen stored key count.
///
/// Stock tables contain zero-key rows for unused parameter channels. They are
/// retained here and become an error only if live environment selection tries
/// to sample that exact row.
fn band_entries(table: &WdbcTable, row: u32) -> Result<usize, AssetError> {
    let entries = field(table, row, 1)? as usize;
    if entries <= BAND_KEY_COUNT {
        return Ok(entries);
    }
    Err(database_error(
        table,
        format!("record {row} has {entries} band keys; expected at most 16"),
    ))
}

/// Reads all time keys, including padded values retained by the table.
fn band_times(table: &WdbcTable, row: u32) -> Result<[u32; BAND_KEY_COUNT], AssetError> {
    read_fields(table, row, 2)
}

/// Reads one adjacent fixed-size field group.
fn read_fields<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first: u32,
) -> Result<[u32; N], AssetError> {
    let mut values = [0; N];
    for (index, value) in values.iter_mut().enumerate() {
        *value = field(table, row, first + index as u32)?;
    }
    Ok(values)
}

/// Reads one checked field from the fixed record.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Reads one finite IEEE-754 field.
fn float_field(table: &WdbcTable, row: u32, column: u32) -> Result<f32, AssetError> {
    let value = f32::from_bits(field(table, row, column)?);
    if value.is_finite() {
        return Ok(value);
    }
    Err(database_error(
        table,
        format!("record {row} field {column} is not finite"),
    ))
}

/// Rejects repeated primary keys while retaining authored Light.dbc order.
fn reject_duplicate(
    table: &WdbcTable,
    ids: &mut HashMap<u32, u32>,
    id: u32,
    row: u32,
) -> Result<(), AssetError> {
    if ids.insert(id, row).is_none() {
        return Ok(());
    }
    Err(database_error(
        table,
        format!("duplicate primary key {id} at record {row}"),
    ))
}

/// Adds the selected DBC path to a schema or record failure.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
