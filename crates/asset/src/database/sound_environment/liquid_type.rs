//! Complete build-12340 `LiquidType.dbc` rows used by liquid sound queries.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::localized::database_error;
use super::super::wow_client_db::WdbcTable;

const LIQUID_TYPE_PATH: &str = "DBFilesClient\\LiquidType.dbc";
const LIQUID_TYPE_FIELD_COUNT: u32 = 45;

/// One exact liquid material, presentation, and sound definition.
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidTypeDefinition {
    id: u32,
    name: String,
    flags: u32,
    sound_bank: u32,
    sound_entry_id: u32,
    spell_id: u32,
    darken: [f32; 4],
    light_id: u32,
    particle_scale: f32,
    particle_movement: u32,
    particle_texture_slots: u32,
    material_id: u32,
    textures: [String; 6],
    colors: [u32; 2],
    float_parameters: [f32; 18],
    integer_parameters: [u32; 4],
}

impl LiquidTypeDefinition {
    /// Returns the liquid query identifier stored by terrain and WMO geometry.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored diagnostic name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the complete stock liquid behavior flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the authored sound-bank discriminator.
    #[must_use]
    pub const fn sound_bank(&self) -> u32 {
        self.sound_bank
    }

    /// Returns the base `SoundEntries.dbc` identifier used by liquid audio.
    #[must_use]
    pub const fn sound_entry_id(&self) -> u32 {
        self.sound_entry_id
    }

    /// Returns the liquid's associated spell identifier.
    #[must_use]
    pub const fn spell_id(&self) -> u32 {
        self.spell_id
    }

    /// Returns max depth followed by fog, ambient, and directional darkening.
    #[must_use]
    pub const fn darken_parameters(&self) -> [f32; 4] {
        self.darken
    }

    /// Returns the direct `LightParams.dbc` override, or zero for world light banks.
    #[must_use]
    pub const fn light_id(&self) -> u32 {
        self.light_id
    }

    /// Returns the authored particle scale.
    #[must_use]
    pub const fn particle_scale(&self) -> f32 {
        self.particle_scale
    }

    /// Returns the particle movement discriminator.
    #[must_use]
    pub const fn particle_movement(&self) -> u32 {
        self.particle_movement
    }

    /// Returns the native atlas pattern index (0 through 4 in 79CA70).
    #[must_use]
    pub const fn particle_texture_slots(&self) -> u32 {
        self.particle_texture_slots
    }

    /// Returns the related `LiquidMaterial.dbc` identifier.
    #[must_use]
    pub const fn material_id(&self) -> u32 {
        self.material_id
    }

    /// Returns the six authored texture strings in table order.
    #[must_use]
    pub fn textures(&self) -> &[String; 6] {
        &self.textures
    }

    /// Returns the two authored packed color words.
    #[must_use]
    pub const fn colors(&self) -> [u32; 2] {
        self.colors
    }

    /// Returns the 18 presentation floats whose meanings vary by liquid shader.
    #[must_use]
    pub const fn float_parameters(&self) -> [f32; 18] {
        self.float_parameters
    }

    /// Returns the final four stock integer parameters.
    #[must_use]
    pub const fn integer_parameters(&self) -> [u32; 4] {
        self.integer_parameters
    }
}

/// Identifier-indexed build-12340 liquid definitions.
pub struct LiquidTypeCatalog {
    entries: Vec<LiquidTypeDefinition>,
}

impl LiquidTypeCatalog {
    /// Loads the exact 45-word build-12340 layout through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, schema, string, finite-float, or
    /// duplicate-primary-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(LIQUID_TYPE_PATH)?)?;
        require_layout(&table)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            entries.push(LiquidTypeDefinition {
                id: field(&table, row, 0)?,
                name: string(&table, row, 1)?,
                flags: field(&table, row, 2)?,
                sound_bank: field(&table, row, 3)?,
                sound_entry_id: field(&table, row, 4)?,
                spell_id: field(&table, row, 5)?,
                darken: finite_float_array(&table, row, 6)?,
                light_id: field(&table, row, 10)?,
                particle_scale: finite_float(&table, row, 11)?,
                particle_movement: field(&table, row, 12)?,
                particle_texture_slots: field(&table, row, 13)?,
                material_id: field(&table, row, 14)?,
                textures: string_array(&table, row, 15)?,
                colors: [field(&table, row, 21)?, field(&table, row, 22)?],
                float_parameters: finite_float_array(&table, row, 23)?,
                integer_parameters: unsigned_array(&table, row, 41)?,
            });
        }
        entries.sort_unstable_by_key(LiquidTypeDefinition::id);
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds one exact liquid type identifier.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&LiquidTypeDefinition> {
        self.entries
            .binary_search_by_key(&id, LiquidTypeDefinition::id)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Returns every row in ascending identifier order.
    #[must_use]
    pub fn entries(&self) -> &[LiquidTypeDefinition] {
        &self.entries
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == LIQUID_TYPE_FIELD_COUNT
        && header.record_size() == LIQUID_TYPE_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 LiquidType.dbc requires 45 fields and 180-byte records; found {} fields and {}-byte records",
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

fn finite_float(table: &WdbcTable, row: u32, column: u32) -> Result<f32, AssetError> {
    let value = f32::from_bits(field(table, row, column)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(database_error(
            table,
            format!("record {row} field {column} is not finite"),
        ))
    }
}

fn finite_float_array<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first_column: u32,
) -> Result<[f32; N], AssetError> {
    let mut values = [0.0; N];
    for (index, value) in values.iter_mut().enumerate() {
        *value = finite_float(table, row, first_column + index as u32)?;
    }
    Ok(values)
}

fn unsigned_array<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first_column: u32,
) -> Result<[u32; N], AssetError> {
    let mut values = [0; N];
    for (index, value) in values.iter_mut().enumerate() {
        *value = field(table, row, first_column + index as u32)?;
    }
    Ok(values)
}

fn string_array<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first_column: u32,
) -> Result<[String; N], AssetError> {
    let mut values = std::array::from_fn(|_| String::new());
    for (index, value) in values.iter_mut().enumerate() {
        *value = string(table, row, first_column + index as u32)?;
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
