//! Build-12340 sound identifiers and their ordered archive payloads.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const SOUND_ENTRIES_PATH: &str = "DBFilesClient\\SoundEntries.dbc";
const SOUND_ENTRIES_FIELD_COUNT: u32 = 30;
const VARIATION_COUNT: u32 = 10;
const FIRST_FILE_FIELD: u32 = 3;
const FIRST_FREQUENCY_FIELD: u32 = 13;
const DIRECTORY_FIELD: u32 = 23;

/// One authored file variation and its stock selection weight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoundAsset {
    path: AssetPath,
    frequency: u32,
}

impl SoundAsset {
    /// Returns the normalized MPQ identity selected by ordinary precedence.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the unmodified `Freq` value paired with this variation.
    #[must_use]
    pub const fn frequency(&self) -> u32 {
        self.frequency
    }
}

/// One exact `SoundEntries.dbc` row used by client audio policy.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEntry {
    id: u32,
    sound_type: u32,
    internal_name: String,
    assets: Vec<SoundAsset>,
    volume: f32,
    flags: u32,
    minimum_distance: f32,
    distance_cutoff: f32,
    eax_definition_id: u32,
    advanced_id: u32,
}

impl SoundEntry {
    /// Returns the client sound identifier referenced by other stock tables.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the unmodified `SoundType` classification.
    #[must_use]
    pub const fn sound_type(&self) -> u32 {
        self.sound_type
    }

    /// Returns the authored non-localized diagnostic and script name.
    #[must_use]
    pub fn internal_name(&self) -> &str {
        &self.internal_name
    }

    /// Returns nonempty file slots in their authored zero-through-nine order.
    #[must_use]
    pub fn assets(&self) -> &[SoundAsset] {
        &self.assets
    }

    /// Returns the authored per-entry gain multiplier.
    #[must_use]
    pub const fn volume(&self) -> f32 {
        self.volume
    }

    /// Returns all stock sound flags without reinterpretation.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the authored near attenuation distance.
    #[must_use]
    pub const fn minimum_distance(&self) -> f32 {
        self.minimum_distance
    }

    /// Returns the authored distance at which the sound is culled.
    #[must_use]
    pub const fn distance_cutoff(&self) -> f32 {
        self.distance_cutoff
    }

    /// Returns the legacy environmental-audio definition identifier.
    #[must_use]
    pub const fn eax_definition_id(&self) -> u32 {
        self.eax_definition_id
    }

    /// Returns the associated `SoundEntriesAdvanced.dbc` identifier.
    #[must_use]
    pub const fn advanced_id(&self) -> u32 {
        self.advanced_id
    }
}

/// Identifier-indexed build-12340 sound entry table.
pub struct SoundEntryCatalog {
    entries: Vec<SoundEntry>,
}

impl SoundEntryCatalog {
    /// Loads the exact 30-word layout through ordinary archive precedence.
    ///
    /// Empty file slots remain absent rather than receiving an invented media
    /// path. The remaining slots retain their authored order and frequencies.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, schema, string, path, float, or
    /// duplicate-primary-key failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(SOUND_ENTRIES_PATH)?)?;
        require_layout(&table)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let directory = string(&table, row, DIRECTORY_FIELD)?;
            let mut assets = Vec::with_capacity(VARIATION_COUNT as usize);
            for variation in 0..VARIATION_COUNT {
                let file = string(&table, row, FIRST_FILE_FIELD + variation)?;
                if file.is_empty() {
                    continue;
                }
                let authored_path = if directory.is_empty() {
                    file
                } else if directory.ends_with(['\\', '/']) {
                    format!("{directory}{file}")
                } else {
                    format!("{directory}\\{file}")
                };
                // Shipped build-12340 rows include root-relative names and one
                // leaked UNC build-machine name. The client archive boundary
                // removes every leading root marker before MPQ lookup. The UNC
                // payload therefore remains an ordinary, unresolvable archive
                // identity instead of becoming a host-filesystem fallback.
                let archive_path = authored_path.trim_start_matches(['\\', '/']);
                let path = AssetPath::new(archive_path).map_err(|source| {
                    database_error(
                        &table,
                        format!(
                            "record {row} file slot {variation} has invalid path {authored_path:?}: {source}"
                        ),
                    )
                })?;
                assets.push(SoundAsset {
                    path,
                    frequency: field(&table, row, FIRST_FREQUENCY_FIELD + variation)?,
                });
            }
            entries.push(SoundEntry {
                id: field(&table, row, 0)?,
                sound_type: field(&table, row, 1)?,
                internal_name: string(&table, row, 2)?,
                assets,
                volume: finite_float(&table, row, 24)?,
                flags: field(&table, row, 25)?,
                minimum_distance: finite_float(&table, row, 26)?,
                distance_cutoff: finite_float(&table, row, 27)?,
                eax_definition_id: field(&table, row, 28)?,
                advanced_id: field(&table, row, 29)?,
            });
        }
        entries.sort_unstable_by_key(SoundEntry::id);
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { entries })
    }

    /// Finds one exact client sound identifier.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&SoundEntry> {
        self.entries
            .binary_search_by_key(&id, SoundEntry::id)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Returns every definition in ascending identifier order.
    #[must_use]
    pub fn entries(&self) -> &[SoundEntry] {
        &self.entries
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == SOUND_ENTRIES_FIELD_COUNT
        && header.record_size() == SOUND_ENTRIES_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 SoundEntries.dbc requires 30 fields and 120-byte records; found {} fields and {}-byte records",
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

fn finite_float(table: &WdbcTable, row: u32, column: u32) -> Result<f32, AssetError> {
    let value = f32::from_bits(field(table, row, column)?);
    if value.is_finite() {
        return Ok(value);
    }
    Err(database_error(
        table,
        format!("record {row} field {column} is not finite"),
    ))
}
