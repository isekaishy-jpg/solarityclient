//! Exact build-12340 character race names used by component-model paths.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::localized_string;
use super::wow_client_db::WdbcTable;

const CHARACTER_RACES_PATH: &str = "DBFilesClient\\ChrRaces.dbc";
const CHARACTER_RACES_FIELD_COUNT: u32 = 69;

/// One player race's stock file-naming and body-display identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterRace {
    id: u32,
    flags: u32,
    faction_id: u32,
    male_display_id: u32,
    female_display_id: u32,
    splash_sound_id: u32,
    client_prefix: String,
    client_file_string: String,
    name: String,
    female_name: String,
    male_name: String,
    facial_hair_customization: [String; 2],
    hair_customization: String,
    required_expansion: u32,
}

impl CharacterRace {
    /// Returns the identifier carried by unit race bytes and appearance tables.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the complete build-12340 race flags word.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the `FactionTemplate.dbc` identifier used to classify the race.
    #[must_use]
    pub const fn faction_id(&self) -> u32 {
        self.faction_id
    }

    /// Returns the male player creature-display identifier.
    #[must_use]
    pub const fn male_display_id(&self) -> u32 {
        self.male_display_id
    }

    /// Returns the female player creature-display identifier.
    #[must_use]
    pub const fn female_display_id(&self) -> u32 {
        self.female_display_id
    }

    /// Returns field 10's SoundEntries identifier copied to Unit_C +0x8F0.
    #[must_use]
    pub const fn splash_sound_id(&self) -> u32 {
        self.splash_sound_id
    }

    /// Returns the short helmet-model suffix such as `Hu` or `Or`.
    #[must_use]
    pub fn client_prefix(&self) -> &str {
        &self.client_prefix
    }

    /// Returns the race's stock character-directory filename component.
    #[must_use]
    pub fn client_file_string(&self) -> &str {
        &self.client_file_string
    }

    /// Returns the exact selected-locale race name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the selected-locale female display name when authored.
    #[must_use]
    pub fn female_name(&self) -> &str {
        &self.female_name
    }

    /// Returns the selected-locale male display name when authored.
    #[must_use]
    pub fn male_name(&self) -> &str {
        &self.male_name
    }

    /// Returns the sex-specific facial-feature label token.
    #[must_use]
    pub fn facial_hair_customization(&self, gender_id: u8) -> Option<&str> {
        self.facial_hair_customization
            .get(usize::from(gender_id))
            .map(String::as_str)
    }

    /// Returns the race's hair-style label token.
    #[must_use]
    pub fn hair_customization(&self) -> &str {
        &self.hair_customization
    }

    /// Returns the minimum account expansion required to select the race.
    #[must_use]
    pub const fn required_expansion(&self) -> u32 {
        self.required_expansion
    }

    /// Selects the exact stock display-name column for one binary gender.
    #[must_use]
    pub fn display_name(&self, gender_id: u8) -> &str {
        let authored = match gender_id {
            0 => &self.male_name,
            1 => &self.female_name,
            _ => "",
        };
        if authored.is_empty() {
            &self.name
        } else {
            authored
        }
    }
}

/// Identifier-indexed `ChrRaces.dbc` records used by character rendering.
pub struct CharacterRaceCatalog {
    races: Vec<CharacterRace>,
    physical_order: Vec<usize>,
}

impl CharacterRaceCatalog {
    /// Loads the exact 69-word build-12340 race table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, belongs to
    /// another build, contains invalid file strings, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let path = AssetPath::new(CHARACTER_RACES_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut races = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            races.push(CharacterRace {
                id: field(&table, row, 0)?,
                flags: field(&table, row, 1)?,
                faction_id: field(&table, row, 2)?,
                male_display_id: field(&table, row, 4)?,
                female_display_id: field(&table, row, 5)?,
                splash_sound_id: field(&table, row, 10)?,
                client_prefix: string(&table, row, 6)?,
                client_file_string: string(&table, row, 11)?,
                name: localized_string(&table, row, 14, locale)?,
                female_name: localized_string(&table, row, 31, locale)?,
                male_name: localized_string(&table, row, 48, locale)?,
                facial_hair_customization: [string(&table, row, 65)?, string(&table, row, 66)?],
                hair_customization: string(&table, row, 67)?,
                required_expansion: field(&table, row, 68)?,
            });
        }
        let physical_ids = races.iter().map(CharacterRace::id).collect::<Vec<_>>();
        races.sort_unstable_by_key(CharacterRace::id);
        if let Some(duplicate) = races.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        let physical_order = physical_ids
            .into_iter()
            .map(|id| {
                races
                    .binary_search_by_key(&id, CharacterRace::id)
                    .map_err(|_source| database_error(&table, format!("lost race key {id}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            races,
            physical_order,
        })
    }

    /// Finds one exact race identifier without substituting another row.
    #[must_use]
    pub fn race(&self, id: u32) -> Option<&CharacterRace> {
        self.races
            .binary_search_by_key(&id, CharacterRace::id)
            .ok()
            .map(|index| &self.races[index])
    }

    /// Iterates records in ascending race identifier order.
    pub fn races(&self) -> impl ExactSizeIterator<Item = &CharacterRace> {
        self.races.iter()
    }

    /// Iterates records in physical DBC order, matching stock creation setup.
    pub fn physical_races(&self) -> impl ExactSizeIterator<Item = &CharacterRace> {
        self.physical_order.iter().map(|index| &self.races[*index])
    }
}

/// Rejects another build before interpreting sparse and localized columns.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == CHARACTER_RACES_FIELD_COUNT
        && header.record_size() == CHARACTER_RACES_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 ChrRaces.dbc requires 69 fields and 276-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one required physical field with record and column context.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Reads one ASCII file-naming string from the table string block.
fn string(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    if !bytes.is_ascii() || bytes.contains(&0) {
        return Err(database_error(
            table,
            format!("record {row} field {column} contains an invalid race file string"),
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
