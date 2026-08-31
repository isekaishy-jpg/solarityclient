//! Complete build-12340 SoundEntriesAdvanced policy rows.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const SOUND_ENTRIES_ADVANCED_PATH: &str = "DBFilesClient\\SoundEntriesAdvanced.dbc";
const SOUND_ENTRIES_ADVANCED_FIELD_COUNT: u32 = 24;

/// One exact `SoundEntriesAdvanced.dbc` row without inferred enum semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedSoundEntry {
    id: u32,
    sound_entry_id: u32,
    inner_radius_2d: f32,
    times: [u32; 4],
    random_offset_range: u32,
    usage: u32,
    time_interval_minimum: u32,
    time_interval_maximum: u32,
    volume_slider_category: u32,
    duck_to_sfx: f32,
    duck_to_music: f32,
    duck_to_ambience: f32,
    inner_radius_of_influence: f32,
    outer_radius_of_influence: f32,
    time_to_duck: u32,
    time_to_unduck: u32,
    inside_angle: f32,
    outside_angle: f32,
    outside_volume: f32,
    outer_radius_2d: f32,
    name: String,
}

impl AdvancedSoundEntry {
    /// Returns the identifier referenced by `SoundEntriesAdvancedID`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the row's explicit `SoundEntries.dbc` relation.
    #[must_use]
    pub const fn sound_entry_id(&self) -> u32 {
        self.sound_entry_id
    }

    /// Returns the authored inner two-dimensional radius.
    #[must_use]
    pub const fn inner_radius_2d(&self) -> f32 {
        self.inner_radius_2d
    }

    /// Returns the unmodified `TimeA` through `TimeD` values.
    #[must_use]
    pub const fn times(&self) -> [u32; 4] {
        self.times
    }

    /// Returns the unmodified random-offset range.
    #[must_use]
    pub const fn random_offset_range(&self) -> u32 {
        self.random_offset_range
    }

    /// Returns the unmodified usage discriminator.
    #[must_use]
    pub const fn usage(&self) -> u32 {
        self.usage
    }

    /// Returns the authored minimum repeat interval.
    #[must_use]
    pub const fn time_interval_minimum(&self) -> u32 {
        self.time_interval_minimum
    }

    /// Returns the authored maximum repeat interval.
    #[must_use]
    pub const fn time_interval_maximum(&self) -> u32 {
        self.time_interval_maximum
    }

    /// Returns the unmodified volume-slider category discriminator.
    #[must_use]
    pub const fn volume_slider_category(&self) -> u32 {
        self.volume_slider_category
    }

    /// Returns the authored SFX ducking multiplier.
    #[must_use]
    pub const fn duck_to_sfx(&self) -> f32 {
        self.duck_to_sfx
    }

    /// Returns the authored music ducking multiplier.
    #[must_use]
    pub const fn duck_to_music(&self) -> f32 {
        self.duck_to_music
    }

    /// Returns the authored ambience ducking multiplier.
    #[must_use]
    pub const fn duck_to_ambience(&self) -> f32 {
        self.duck_to_ambience
    }

    /// Returns the inner radius used by influence policy.
    #[must_use]
    pub const fn inner_radius_of_influence(&self) -> f32 {
        self.inner_radius_of_influence
    }

    /// Returns the outer radius used by influence policy.
    #[must_use]
    pub const fn outer_radius_of_influence(&self) -> f32 {
        self.outer_radius_of_influence
    }

    /// Returns the authored duck transition time.
    #[must_use]
    pub const fn time_to_duck(&self) -> u32 {
        self.time_to_duck
    }

    /// Returns the authored unduck transition time.
    #[must_use]
    pub const fn time_to_unduck(&self) -> u32 {
        self.time_to_unduck
    }

    /// Returns the authored inner directional-cone angle.
    #[must_use]
    pub const fn inside_angle(&self) -> f32 {
        self.inside_angle
    }

    /// Returns the authored outer directional-cone angle.
    #[must_use]
    pub const fn outside_angle(&self) -> f32 {
        self.outside_angle
    }

    /// Returns gain outside the authored directional cone.
    #[must_use]
    pub const fn outside_volume(&self) -> f32 {
        self.outside_volume
    }

    /// Returns the authored outer two-dimensional radius.
    #[must_use]
    pub const fn outer_radius_2d(&self) -> f32 {
        self.outer_radius_2d
    }

    /// Returns the authored diagnostic name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Identifier-indexed build-12340 advanced sound policy table.
pub struct AdvancedSoundEntryCatalog {
    entries: Vec<AdvancedSoundEntry>,
}

impl AdvancedSoundEntryCatalog {
    /// Loads the exact 24-word build-12340 layout through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, schema, text, finite-float, or
    /// duplicate-primary-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(SOUND_ENTRIES_ADVANCED_PATH)?)?;
        require_layout(&table)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            entries.push(AdvancedSoundEntry {
                id: field(&table, row, 0)?,
                sound_entry_id: field(&table, row, 1)?,
                inner_radius_2d: finite_float(&table, row, 2)?,
                times: [
                    field(&table, row, 3)?,
                    field(&table, row, 4)?,
                    field(&table, row, 5)?,
                    field(&table, row, 6)?,
                ],
                random_offset_range: field(&table, row, 7)?,
                usage: field(&table, row, 8)?,
                time_interval_minimum: field(&table, row, 9)?,
                time_interval_maximum: field(&table, row, 10)?,
                volume_slider_category: field(&table, row, 11)?,
                duck_to_sfx: finite_float(&table, row, 12)?,
                duck_to_music: finite_float(&table, row, 13)?,
                duck_to_ambience: finite_float(&table, row, 14)?,
                inner_radius_of_influence: finite_float(&table, row, 15)?,
                outer_radius_of_influence: finite_float(&table, row, 16)?,
                time_to_duck: field(&table, row, 17)?,
                time_to_unduck: field(&table, row, 18)?,
                inside_angle: finite_float(&table, row, 19)?,
                outside_angle: finite_float(&table, row, 20)?,
                outside_volume: finite_float(&table, row, 21)?,
                outer_radius_2d: finite_float(&table, row, 22)?,
                name: string(&table, row, 23)?,
            });
        }
        entries.sort_unstable_by_key(AdvancedSoundEntry::id);
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds one exact advanced-policy identifier.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&AdvancedSoundEntry> {
        self.entries
            .binary_search_by_key(&id, AdvancedSoundEntry::id)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Returns every row in ascending identifier order.
    #[must_use]
    pub fn entries(&self) -> &[AdvancedSoundEntry] {
        &self.entries
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == SOUND_ENTRIES_ADVANCED_FIELD_COUNT
        && header.record_size() == SOUND_ENTRIES_ADVANCED_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 SoundEntriesAdvanced.dbc requires 24 fields and 96-byte records; found {} fields and {}-byte records",
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
