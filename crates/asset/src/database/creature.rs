//! Typed build-12340 creature display and model database catalogs.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::wow_client_db::WdbcTable;

const DISPLAY_INFO_PATH: &str = "DBFilesClient\\CreatureDisplayInfo.dbc";
const DISPLAY_INFO_EXTRA_PATH: &str = "DBFilesClient\\CreatureDisplayInfoExtra.dbc";
const MODEL_DATA_PATH: &str = "DBFilesClient\\CreatureModelData.dbc";
const FAMILY_PATH: &str = "DBFilesClient\\CreatureFamily.dbc";
const DISPLAY_FIELD_COUNT: u32 = 16;
const DISPLAY_EXTRA_FIELD_COUNT: u32 = 21;
const MODEL_FIELD_COUNT: u32 = 28;
const FAMILY_FIELD_COUNT: u32 = 28;

/// One exact build-12340 `CreatureFamily.dbc` scale interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreatureFamilyDefinition {
    id: u32,
    minimum_scale: f32,
    minimum_scale_level: u32,
    maximum_scale: f32,
    maximum_scale_level: u32,
}

impl CreatureFamilyDefinition {
    /// Returns the creature-family identifier carried by character enumeration.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored minimum scale and its level.
    #[must_use]
    pub const fn minimum_scale(&self) -> (f32, u32) {
        (self.minimum_scale, self.minimum_scale_level)
    }

    /// Returns the authored maximum scale and its level.
    #[must_use]
    pub const fn maximum_scale(&self) -> (f32, u32) {
        (self.maximum_scale, self.maximum_scale_level)
    }

    /// Interpolates the stock pet scale after clamping level to the authored interval.
    #[must_use]
    pub fn scale_for_level(&self, level: u32) -> Option<f32> {
        if self.maximum_scale_level <= self.minimum_scale_level {
            return None;
        }
        let level = level.clamp(self.minimum_scale_level, self.maximum_scale_level);
        let progress = (level - self.minimum_scale_level) as f32
            / (self.maximum_scale_level - self.minimum_scale_level) as f32;
        let scale = self.minimum_scale + (self.maximum_scale - self.minimum_scale) * progress;
        scale
            .is_finite()
            .then_some(scale)
            .filter(|scale| *scale > 0.0)
    }
}

/// Sorted creature-family scale rows used by stock character-selection pets.
#[derive(Default)]
pub struct CreatureFamilyCatalog {
    families: Vec<CreatureFamilyDefinition>,
}

impl CreatureFamilyCatalog {
    /// Loads the exact five-field build-12340 creature-family table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, has another layout,
    /// contains a non-finite scale, or repeats a primary key.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(FAMILY_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        Ok(Self {
            families: decode_families(&table)?,
        })
    }

    /// Finds a family row in logarithmic time by its exact identifier.
    #[must_use]
    pub fn family(&self, id: u32) -> Option<CreatureFamilyDefinition> {
        self.families
            .binary_search_by_key(&id, CreatureFamilyDefinition::id)
            .ok()
            .map(|index| self.families[index])
    }
}

/// One exact build-12340 `CreatureDisplayInfo.dbc` row.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureDisplayInfo {
    id: u32,
    model_id: u32,
    sound_id: u32,
    extended_display_info_id: u32,
    model_scale: f32,
    model_alpha: u32,
    texture_variations: [String; 3],
    portrait_texture_name: String,
    size_class: u32,
    blood_id: u32,
    npc_sound_id: u32,
    particle_color_id: u32,
    geoset_data: u32,
    object_effect_package_id: u32,
}

impl CreatureDisplayInfo {
    /// Returns the identifier carried by `UNIT_FIELD_DISPLAYID`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the referenced `CreatureModelData.dbc` row identifier.
    #[must_use]
    pub const fn model_id(&self) -> u32 {
        self.model_id
    }

    /// Returns the referenced creature sound row.
    #[must_use]
    pub const fn sound_id(&self) -> u32 {
        self.sound_id
    }

    /// Returns the optional `CreatureDisplayInfoExtra.dbc` row identifier.
    #[must_use]
    pub const fn extended_display_info_id(&self) -> u32 {
        self.extended_display_info_id
    }

    /// Returns the authored display scale without substituting a default.
    #[must_use]
    pub const fn model_scale(&self) -> f32 {
        self.model_scale
    }

    /// Returns the authored model alpha word.
    #[must_use]
    pub const fn model_alpha(&self) -> u32 {
        self.model_alpha
    }

    /// Returns the three skin names in their stock replacement order.
    #[must_use]
    pub fn texture_variations(&self) -> [&str; 3] {
        self.texture_variations.each_ref().map(String::as_str)
    }

    /// Returns the optional portrait texture name; an empty string is retained.
    #[must_use]
    pub fn portrait_texture_name(&self) -> &str {
        &self.portrait_texture_name
    }

    /// Returns the stock size classification.
    #[must_use]
    pub const fn size_class(&self) -> u32 {
        self.size_class
    }

    /// Returns the UnitBlood.dbc identifier.
    #[must_use]
    pub const fn blood_id(&self) -> u32 {
        self.blood_id
    }

    /// Returns the NPCSounds.dbc identifier.
    #[must_use]
    pub const fn npc_sound_id(&self) -> u32 {
        self.npc_sound_id
    }

    /// Returns the ParticleColor.dbc identifier.
    #[must_use]
    pub const fn particle_color_id(&self) -> u32 {
        self.particle_color_id
    }

    /// Returns the packed creature geoset selection word.
    #[must_use]
    pub const fn geoset_data(&self) -> u32 {
        self.geoset_data
    }

    /// Returns the ObjectEffectPackage.dbc identifier.
    #[must_use]
    pub const fn object_effect_package_id(&self) -> u32 {
        self.object_effect_package_id
    }
}

/// One exact build-12340 `CreatureDisplayInfoExtra.dbc` appearance row.
///
/// Later clients add HD-specific file identifiers to this table. Build 12340
/// does not: higher-quality replacements continue to resolve through the same
/// baked texture path and normal MPQ precedence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatureDisplayInfoExtra {
    id: u32,
    race_id: u32,
    gender_id: u32,
    skin_id: u32,
    face_id: u32,
    hair_style_id: u32,
    hair_color_id: u32,
    facial_hair_style_id: u32,
    npc_item_display_ids: [u32; 11],
    flags: u32,
    baked_texture_name: String,
}

impl CreatureDisplayInfoExtra {
    /// Returns the identifier referenced by a creature display row.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the ChrRaces.dbc identifier.
    #[must_use]
    pub const fn race_id(&self) -> u32 {
        self.race_id
    }

    /// Returns the stock gender identifier.
    #[must_use]
    pub const fn gender_id(&self) -> u32 {
        self.gender_id
    }

    /// Returns the skin customization identifier.
    #[must_use]
    pub const fn skin_id(&self) -> u32 {
        self.skin_id
    }

    /// Returns the face customization identifier.
    #[must_use]
    pub const fn face_id(&self) -> u32 {
        self.face_id
    }

    /// Returns the hair-style customization identifier.
    #[must_use]
    pub const fn hair_style_id(&self) -> u32 {
        self.hair_style_id
    }

    /// Returns the hair-color customization identifier.
    #[must_use]
    pub const fn hair_color_id(&self) -> u32 {
        self.hair_color_id
    }

    /// Returns the facial-hair customization identifier.
    #[must_use]
    pub const fn facial_hair_style_id(&self) -> u32 {
        self.facial_hair_style_id
    }

    /// Returns all eleven NPC equipment display slots in stock order.
    #[must_use]
    pub const fn npc_item_display_ids(&self) -> [u32; 11] {
        self.npc_item_display_ids
    }

    /// Returns the exact appearance flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the baked character texture name, preserving authored absence.
    #[must_use]
    pub fn baked_texture_name(&self) -> &str {
        &self.baked_texture_name
    }
}

/// One exact build-12340 `CreatureModelData.dbc` row.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureModelData {
    id: u32,
    flags: u32,
    model_path: Option<AssetPath>,
    size_class: u32,
    model_scale: f32,
    blood_id: u32,
    footprint_texture_id: u32,
    footprint_texture_length: f32,
    footprint_texture_width: f32,
    footprint_particle_scale: f32,
    foley_material_id: u32,
    footstep_shake_size: u32,
    death_thud_shake_size: u32,
    sound_id: u32,
    collision_width: f32,
    collision_height: f32,
    mount_height: f32,
    geometry_box_min: [f32; 3],
    geometry_box_max: [f32; 3],
    world_effect_scale: f32,
    attached_effect_scale: f32,
    missile_collision_radius: f32,
    missile_collision_push: f32,
    missile_collision_raise: f32,
}

impl CreatureModelData {
    /// Returns the identifier referenced by creature display rows.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the exact model flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the normalized model asset path, preserving authored absence.
    #[must_use]
    pub const fn model_path(&self) -> Option<&AssetPath> {
        self.model_path.as_ref()
    }

    /// Returns the stock size classification.
    #[must_use]
    pub const fn size_class(&self) -> u32 {
        self.size_class
    }

    /// Returns the authored model scale without substituting a default.
    #[must_use]
    pub const fn model_scale(&self) -> f32 {
        self.model_scale
    }

    /// Returns the UnitBlood.dbc identifier.
    #[must_use]
    pub const fn blood_id(&self) -> u32 {
        self.blood_id
    }

    /// Returns the footprint texture identifier.
    #[must_use]
    pub const fn footprint_texture_id(&self) -> u32 {
        self.footprint_texture_id
    }

    /// Returns footprint length and width in authored world units.
    #[must_use]
    pub const fn footprint_extent(&self) -> [f32; 2] {
        [self.footprint_texture_length, self.footprint_texture_width]
    }

    /// Returns the authored footprint-particle scale.
    #[must_use]
    pub const fn footprint_particle_scale(&self) -> f32 {
        self.footprint_particle_scale
    }

    /// Returns the Material.dbc identifier used for foley selection.
    #[must_use]
    pub const fn foley_material_id(&self) -> u32 {
        self.foley_material_id
    }

    /// Returns the stock footstep camera-shake size.
    #[must_use]
    pub const fn footstep_shake_size(&self) -> u32 {
        self.footstep_shake_size
    }

    /// Returns the stock death-thud camera-shake size.
    #[must_use]
    pub const fn death_thud_shake_size(&self) -> u32 {
        self.death_thud_shake_size
    }

    /// Returns the CreatureSoundData.dbc identifier.
    #[must_use]
    pub const fn sound_id(&self) -> u32 {
        self.sound_id
    }

    /// Returns collision width and height in authored world units.
    #[must_use]
    pub const fn collision_extent(&self) -> [f32; 2] {
        [self.collision_width, self.collision_height]
    }

    /// Returns the mount attachment height in authored world units.
    #[must_use]
    pub const fn mount_height(&self) -> f32 {
        self.mount_height
    }

    /// Returns the minimum authored geometry bounds.
    #[must_use]
    pub const fn geometry_box_min(&self) -> [f32; 3] {
        self.geometry_box_min
    }

    /// Returns the maximum authored geometry bounds.
    #[must_use]
    pub const fn geometry_box_max(&self) -> [f32; 3] {
        self.geometry_box_max
    }

    /// Returns the authored world-effect scale.
    #[must_use]
    pub const fn world_effect_scale(&self) -> f32 {
        self.world_effect_scale
    }

    /// Returns the authored attached-effect scale.
    #[must_use]
    pub const fn attached_effect_scale(&self) -> f32 {
        self.attached_effect_scale
    }

    /// Returns the missile collision radius, push, and raise values.
    #[must_use]
    pub const fn missile_collision(&self) -> [f32; 3] {
        [
            self.missile_collision_radius,
            self.missile_collision_push,
            self.missile_collision_raise,
        ]
    }
}

/// Sorted render-facing creature tables loaded through ordinary MPQ precedence.
pub struct CreatureCatalog {
    displays: Vec<CreatureDisplayInfo>,
    display_extras: Vec<CreatureDisplayInfoExtra>,
    models: Vec<CreatureModelData>,
}

impl CreatureCatalog {
    /// Loads and validates the two build-12340 creature model tables.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when either table is missing, has another build's
    /// layout, contains an invalid string/path, or repeats a primary key.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let display_path = AssetPath::new(DISPLAY_INFO_PATH)?;
        let display_extra_path = AssetPath::new(DISPLAY_INFO_EXTRA_PATH)?;
        let model_path = AssetPath::new(MODEL_DATA_PATH)?;
        let display_table = WdbcTable::load(store, &display_path)?;
        let display_extra_table = WdbcTable::load(store, &display_extra_path)?;
        let model_table = WdbcTable::load(store, &model_path)?;
        let displays = decode_displays(&display_table)?;
        let display_extras = decode_display_extras(&display_extra_table)?;
        let models = decode_models(&model_table)?;
        Ok(Self {
            displays,
            display_extras,
            models,
        })
    }

    /// Finds a display row in logarithmic time by its exact identifier.
    #[must_use]
    pub fn display(&self, id: u32) -> Option<&CreatureDisplayInfo> {
        self.displays
            .binary_search_by_key(&id, CreatureDisplayInfo::id)
            .ok()
            .map(|index| &self.displays[index])
    }

    /// Finds a model row in logarithmic time by its exact identifier.
    #[must_use]
    pub fn model(&self, id: u32) -> Option<&CreatureModelData> {
        self.models
            .binary_search_by_key(&id, CreatureModelData::id)
            .ok()
            .map(|index| &self.models[index])
    }

    /// Finds an extended appearance row in logarithmic time.
    #[must_use]
    pub fn display_extra(&self, id: u32) -> Option<&CreatureDisplayInfoExtra> {
        self.display_extras
            .binary_search_by_key(&id, CreatureDisplayInfoExtra::id)
            .ok()
            .map(|index| &self.display_extras[index])
    }

    /// Returns display rows sorted by primary key.
    #[must_use]
    pub fn displays(&self) -> &[CreatureDisplayInfo] {
        &self.displays
    }

    /// Returns model rows sorted by primary key.
    #[must_use]
    pub fn models(&self) -> &[CreatureModelData] {
        &self.models
    }

    /// Returns extended appearance rows sorted by primary key.
    #[must_use]
    pub fn display_extras(&self) -> &[CreatureDisplayInfoExtra] {
        &self.display_extras
    }
}

/// Validates and decodes the exact 16-word display layout.
fn decode_displays(table: &WdbcTable) -> Result<Vec<CreatureDisplayInfo>, AssetError> {
    require_layout(table, DISPLAY_FIELD_COUNT, "CreatureDisplayInfo.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<16>(table, row)?;
        let model_scale = finite_f32(table, row, 4, values[4])?;
        let texture_variations = [
            table_string(table, row, 6, values[6])?,
            table_string(table, row, 7, values[7])?,
            table_string(table, row, 8, values[8])?,
        ];
        records.push(CreatureDisplayInfo {
            id: values[0],
            model_id: values[1],
            sound_id: values[2],
            extended_display_info_id: values[3],
            model_scale,
            model_alpha: values[5],
            texture_variations,
            portrait_texture_name: table_string(table, row, 9, values[9])?,
            size_class: values[10],
            blood_id: values[11],
            npc_sound_id: values[12],
            particle_color_id: values[13],
            geoset_data: values[14],
            object_effect_package_id: values[15],
        });
    }
    sort_unique(table, records, CreatureDisplayInfo::id)
}

/// Validates and decodes the exact 21-word extended appearance layout.
fn decode_display_extras(table: &WdbcTable) -> Result<Vec<CreatureDisplayInfoExtra>, AssetError> {
    require_layout(
        table,
        DISPLAY_EXTRA_FIELD_COUNT,
        "CreatureDisplayInfoExtra.dbc",
    )?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<21>(table, row)?;
        records.push(CreatureDisplayInfoExtra {
            id: values[0],
            race_id: values[1],
            gender_id: values[2],
            skin_id: values[3],
            face_id: values[4],
            hair_style_id: values[5],
            hair_color_id: values[6],
            facial_hair_style_id: values[7],
            npc_item_display_ids: values[8..19].try_into().map_err(|_| {
                database_error(table, format!("record {row} item slots are truncated"))
            })?,
            flags: values[19],
            baked_texture_name: table_string(table, row, 20, values[20])?,
        });
    }
    sort_unique(table, records, CreatureDisplayInfoExtra::id)
}

/// Validates and decodes the exact 28-word model layout.
fn decode_models(table: &WdbcTable) -> Result<Vec<CreatureModelData>, AssetError> {
    require_layout(table, MODEL_FIELD_COUNT, "CreatureModelData.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<28>(table, row)?;
        let model_name = table_string(table, row, 2, values[2])?;
        let model_path = if model_name.is_empty() {
            None
        } else {
            Some(AssetPath::new(&model_name).map_err(|error| {
                database_error(
                    table,
                    format!("record {row} model path is invalid: {error}"),
                )
            })?)
        };
        let floats = |field| finite_f32(table, row, field, values[field as usize]);
        records.push(CreatureModelData {
            id: values[0],
            flags: values[1],
            model_path,
            size_class: values[3],
            model_scale: floats(4)?,
            blood_id: values[5],
            footprint_texture_id: values[6],
            footprint_texture_length: floats(7)?,
            footprint_texture_width: floats(8)?,
            footprint_particle_scale: floats(9)?,
            foley_material_id: values[10],
            footstep_shake_size: values[11],
            death_thud_shake_size: values[12],
            sound_id: values[13],
            collision_width: floats(14)?,
            collision_height: floats(15)?,
            mount_height: floats(16)?,
            geometry_box_min: [floats(17)?, floats(18)?, floats(19)?],
            geometry_box_max: [floats(20)?, floats(21)?, floats(22)?],
            world_effect_scale: floats(23)?,
            attached_effect_scale: floats(24)?,
            missile_collision_radius: floats(25)?,
            missile_collision_push: floats(26)?,
            missile_collision_raise: floats(27)?,
        });
    }
    sort_unique(table, records, CreatureModelData::id)
}

/// Decodes stock's compact family-level pet scale intervals.
fn decode_families(table: &WdbcTable) -> Result<Vec<CreatureFamilyDefinition>, AssetError> {
    require_layout(table, FAMILY_FIELD_COUNT, "CreatureFamily.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<5>(table, row)?;
        records.push(CreatureFamilyDefinition {
            id: values[0],
            minimum_scale: finite_f32(table, row, 1, values[1])?,
            minimum_scale_level: values[2],
            maximum_scale: finite_f32(table, row, 3, values[3])?,
            maximum_scale_level: values[4],
        });
    }
    sort_unique(table, records, CreatureFamilyDefinition::id)
}

/// Rejects another client's schema instead of guessing compatible offsets.
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

/// Copies one fixed-layout row after the schema check established its width.
fn read_fields<const N: usize>(table: &WdbcTable, row: u32) -> Result<[u32; N], AssetError> {
    let mut values = [0_u32; N];
    for (field, value) in values.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(values)
}

/// Decodes an exact finite float without clamping or default substitution.
fn finite_f32(table: &WdbcTable, row: u32, field: u32, value: u32) -> Result<f32, AssetError> {
    let value = f32::from_bits(value);
    if value.is_finite() {
        return Ok(value);
    }
    Err(database_error(
        table,
        format!("record {row} field {field} contains a non-finite float"),
    ))
}

/// Resolves a string-block field while preserving intentional empty strings.
fn table_string(
    table: &WdbcTable,
    row: u32,
    field: u32,
    offset: u32,
) -> Result<String, AssetError> {
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {field} has invalid string offset {offset}"),
        )
    })?;
    if !bytes.is_ascii() {
        return Err(database_error(
            table,
            format!("record {row} field {field} contains a non-ASCII asset name"),
        ));
    }
    let value = String::from_utf8(bytes.to_vec())
        .map_err(|error| database_error(table, error.to_string()))?;
    if !value.is_empty() {
        AssetPath::new(&value).map_err(|error| {
            database_error(
                table,
                format!("record {row} field {field} has invalid asset name: {error}"),
            )
        })?;
    }
    Ok(value)
}

/// Sorts a decoded catalog and rejects ambiguous primary-key ownership.
fn sort_unique<T>(
    table: &WdbcTable,
    mut records: Vec<T>,
    id: impl Fn(&T) -> u32,
) -> Result<Vec<T>, AssetError> {
    records.sort_unstable_by_key(&id);
    if let Some(duplicate) = records.windows(2).find(|pair| id(&pair[0]) == id(&pair[1])) {
        return Err(database_error(
            table,
            format!("duplicate primary key {}", id(&duplicate[0])),
        ));
    }
    Ok(records)
}

/// Adds the selected archive path to a typed table decode failure.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
