//! Area music and ambience relations used by build 12340's zone sound owner.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::localized::database_error;
use super::super::wow_client_db::WdbcTable;

/// Independently inherited sound relations shared by AreaTable and WMOAreaTable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AreaSoundReferences {
    /// Zero inherits the exterior provider from the containing area.
    pub sound_provider_id: u32,
    /// Zero inherits the underwater provider from the containing area.
    pub underwater_sound_provider_id: u32,
    /// Relation to SoundAmbience; zero inherits from the containing area.
    pub ambience_id: u32,
    /// Relation to ZoneMusic; zero inherits from the containing area.
    pub zone_music_id: u32,
    /// Relation to ZoneIntroMusicTable; zero inherits from the containing area.
    pub intro_music_id: u32,
}

impl AreaSoundReferences {
    /// Reads the five consecutive relation words in either area's exact schema.
    pub(in crate::database) fn read(
        table: &WdbcTable,
        row: u32,
        first: u32,
    ) -> Result<Self, AssetError> {
        Ok(Self {
            sound_provider_id: field(table, row, first)?,
            underwater_sound_provider_id: field(table, row, first + 1)?,
            ambience_id: field(table, row, first + 2)?,
            zone_music_id: field(table, row, first + 3)?,
            intro_music_id: field(table, row, first + 4)?,
        })
    }
}

/// One authored music choice for day and night, with delays in milliseconds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZoneMusicDefinition {
    id: u32,
    name: String,
    minimum_delay_ms: [u32; 2],
    maximum_delay_ms: [u32; 2],
    sound_entry_ids: [u32; 2],
}

impl ZoneMusicDefinition {
    /// Returns the ZoneMusic primary key.
    pub const fn id(&self) -> u32 {
        self.id
    }
    /// Returns the authored diagnostic name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns minimum silence after a track finishes, ordered day then night.
    pub const fn minimum_delay_ms(&self) -> [u32; 2] {
        self.minimum_delay_ms
    }
    /// Returns maximum silence after a track finishes, ordered day then night.
    pub const fn maximum_delay_ms(&self) -> [u32; 2] {
        self.maximum_delay_ms
    }
    /// Returns the exact SoundEntries relations, ordered day then night.
    pub const fn sound_entry_ids(&self) -> [u32; 2] {
        self.sound_entry_ids
    }
}

/// One area-entry music cue and its repeat cooldown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZoneIntroMusicDefinition {
    id: u32,
    name: String,
    sound_entry_id: u32,
    priority: u32,
    minimum_delay_minutes: u32,
}

impl ZoneIntroMusicDefinition {
    /// Returns the ZoneIntroMusicTable primary key.
    pub const fn id(&self) -> u32 {
        self.id
    }
    /// Returns the authored diagnostic name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the exact SoundEntries relation.
    pub const fn sound_entry_id(&self) -> u32 {
        self.sound_entry_id
    }
    /// Returns the authored priority field without substituting layer precedence.
    pub const fn priority(&self) -> u32 {
        self.priority
    }
    /// Returns the cooldown started when playback finishes, in whole minutes.
    pub const fn minimum_delay_minutes(&self) -> u32 {
        self.minimum_delay_minutes
    }
}

/// One pair of continuously selected day/night ambience sound entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundAmbienceDefinition {
    id: u32,
    sound_entry_ids: [u32; 2],
}

impl SoundAmbienceDefinition {
    /// Returns the SoundAmbience primary key.
    pub const fn id(&self) -> u32 {
        self.id
    }
    /// Returns the exact SoundEntries relations, ordered day then night.
    pub const fn sound_entry_ids(&self) -> [u32; 2] {
        self.sound_entry_ids
    }
}

/// Exact zone music, introduction, and ambience tables, indexed independently.
pub struct ZoneSoundCatalog {
    music: Vec<ZoneMusicDefinition>,
    intros: Vec<ZoneIntroMusicDefinition>,
    ambience: Vec<SoundAmbienceDefinition>,
}

impl ZoneSoundCatalog {
    /// Loads the 8-, 5-, and 3-word schemas verified against the native loaders.
    ///
    /// # Errors
    /// Returns an archive or schema error, including invalid text and duplicate keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = load_table(store, "DBFilesClient\\ZoneMusic.dbc", 8)?;
        let mut music = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            music.push(ZoneMusicDefinition {
                id: field(&table, row, 0)?,
                name: string(&table, row, 1)?,
                minimum_delay_ms: [field(&table, row, 2)?, field(&table, row, 3)?],
                maximum_delay_ms: [field(&table, row, 4)?, field(&table, row, 5)?],
                sound_entry_ids: [field(&table, row, 6)?, field(&table, row, 7)?],
            });
        }
        sort_unique(&table, &mut music, ZoneMusicDefinition::id)?;
        let table = load_table(store, "DBFilesClient\\ZoneIntroMusicTable.dbc", 5)?;
        let mut intros = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            intros.push(ZoneIntroMusicDefinition {
                id: field(&table, row, 0)?,
                name: string(&table, row, 1)?,
                sound_entry_id: field(&table, row, 2)?,
                priority: field(&table, row, 3)?,
                minimum_delay_minutes: field(&table, row, 4)?,
            });
        }
        sort_unique(&table, &mut intros, ZoneIntroMusicDefinition::id)?;
        let table = load_table(store, "DBFilesClient\\SoundAmbience.dbc", 3)?;
        let mut ambience = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            ambience.push(SoundAmbienceDefinition {
                id: field(&table, row, 0)?,
                sound_entry_ids: [field(&table, row, 1)?, field(&table, row, 2)?],
            });
        }
        sort_unique(&table, &mut ambience, SoundAmbienceDefinition::id)?;
        Ok(Self {
            music,
            intros,
            ambience,
        })
    }

    /// Finds an exact ZoneMusic row; no neighbouring row is substituted.
    pub fn music(&self, id: u32) -> Option<&ZoneMusicDefinition> {
        self.music
            .binary_search_by_key(&id, ZoneMusicDefinition::id)
            .ok()
            .map(|index| &self.music[index])
    }
    /// Finds an exact ZoneIntroMusicTable row.
    pub fn intro(&self, id: u32) -> Option<&ZoneIntroMusicDefinition> {
        self.intros
            .binary_search_by_key(&id, ZoneIntroMusicDefinition::id)
            .ok()
            .map(|index| &self.intros[index])
    }
    /// Finds an exact SoundAmbience row.
    pub fn ambience(&self, id: u32) -> Option<&SoundAmbienceDefinition> {
        self.ambience
            .binary_search_by_key(&id, SoundAmbienceDefinition::id)
            .ok()
            .map(|index| &self.ambience[index])
    }
}

/// Loads an exact all-word schema, preserving archive precedence.
pub(super) fn load_table(
    store: &mut AssetStore,
    path: &str,
    fields: u32,
) -> Result<WdbcTable, AssetError> {
    let table = WdbcTable::load(store, &AssetPath::new(path)?)?;
    if table.header().field_count() != fields || table.header().record_size() != fields * 4 {
        return Err(database_error(
            &table,
            format!(
                "build-12340 schema requires {fields} fields and {}-byte records",
                fields * 4
            ),
        ));
    }
    Ok(table)
}

/// Validates independent primary-key namespaces before publishing a catalog.
pub(super) fn sort_unique<T>(
    table: &WdbcTable,
    rows: &mut [T],
    id: impl Fn(&T) -> u32,
) -> Result<(), AssetError> {
    rows.sort_unstable_by_key(&id);
    if let Some(duplicate) = rows.windows(2).find(|pair| id(&pair[0]) == id(&pair[1])) {
        return Err(database_error(
            table,
            format!("duplicate primary key {}", id(&duplicate[0])),
        ));
    }
    Ok(())
}

/// Reads one required record word without treating truncation as an empty field.
pub(super) fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Decodes authored UTF-8 text with checked string-block offsets.
fn string(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}
