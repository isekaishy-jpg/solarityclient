//! Typed build-12340 player texture and geoset catalogs.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::wow_client_db::WdbcTable;

const CHAR_SECTIONS_PATH: &str = "DBFilesClient\\CharSections.dbc";
const HAIR_GEOSETS_PATH: &str = "DBFilesClient\\CharHairGeosets.dbc";
const FACIAL_HAIR_PATH: &str = "DBFilesClient\\CharacterFacialHairStyles.dbc";

/// One exact build-12340 character texture section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSection {
    id: u32,
    race_id: u32,
    gender_id: u32,
    base_section: u32,
    texture_names: [String; 3],
    flags: u32,
    variation_index: u32,
    color_index: u32,
}

impl CharacterSection {
    /// Returns the table primary key.
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

    /// Returns the stock section category.
    #[must_use]
    pub const fn base_section(&self) -> u32 {
        self.base_section
    }

    /// Returns the three component texture names in layer order.
    #[must_use]
    pub fn texture_names(&self) -> [&str; 3] {
        self.texture_names.each_ref().map(String::as_str)
    }

    /// Returns the exact player/NPC/death-knight applicability flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the customization variation index.
    #[must_use]
    pub const fn variation_index(&self) -> u32 {
        self.variation_index
    }

    /// Returns the customization color index.
    #[must_use]
    pub const fn color_index(&self) -> u32 {
        self.color_index
    }

    /// Returns the compound key used by exact customization lookup.
    const fn key(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.race_id,
            self.gender_id,
            self.base_section,
            self.variation_index,
            self.color_index,
        )
    }
}

/// One exact build-12340 hair geometry selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterHairGeoset {
    id: u32,
    race_id: u32,
    gender_id: u32,
    variation_id: u32,
    geoset_id: u32,
    shows_scalp: u32,
}

impl CharacterHairGeoset {
    /// Returns the table primary key.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the M2 hair geoset selection.
    #[must_use]
    pub const fn geoset_id(self) -> u32 {
        self.geoset_id
    }

    /// Returns the exact authored scalp-visibility word.
    #[must_use]
    pub const fn shows_scalp(self) -> u32 {
        self.shows_scalp
    }

    /// Returns the race, gender, and variation lookup key.
    const fn key(self) -> (u32, u32, u32) {
        (self.race_id, self.gender_id, self.variation_id)
    }
}

/// One exact build-12340 facial-hair geometry selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterFacialHairStyle {
    race_id: u32,
    gender_id: u32,
    variation_id: u32,
    geosets: [u32; 5],
}

impl CharacterFacialHairStyle {
    /// Returns the five facial-hair geoset selections in stock column order.
    #[must_use]
    pub const fn geosets(self) -> [u32; 5] {
        self.geosets
    }

    /// Returns the race, gender, and variation lookup key.
    const fn key(self) -> (u32, u32, u32) {
        (self.race_id, self.gender_id, self.variation_id)
    }
}

/// Indexed texture and geometry choices for build-12340 character models.
pub struct CharacterAppearanceCatalog {
    sections: Vec<CharacterSection>,
    hair_geosets: Vec<CharacterHairGeoset>,
    facial_hair_styles: Vec<CharacterFacialHairStyle>,
}

impl CharacterAppearanceCatalog {
    /// Loads the three stock customization tables through normal MPQ precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for a missing table, mismatched build layout,
    /// invalid texture name, or duplicate exact lookup key.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let section_table = load_table(store, CHAR_SECTIONS_PATH)?;
        let hair_table = load_table(store, HAIR_GEOSETS_PATH)?;
        let facial_table = load_table(store, FACIAL_HAIR_PATH)?;
        Ok(Self {
            sections: decode_sections(&section_table)?,
            hair_geosets: decode_hair_geosets(&hair_table)?,
            facial_hair_styles: decode_facial_hair(&facial_table)?,
        })
    }

    /// Finds every section with an exact customization key.
    ///
    /// Multiple rows are retained because player, NPC, and death-knight flags
    /// can intentionally distinguish records with the same visible choice.
    #[must_use]
    pub fn sections_for(
        &self,
        race_id: u32,
        gender_id: u32,
        base_section: u32,
        variation_index: u32,
        color_index: u32,
    ) -> &[CharacterSection] {
        let key = (
            race_id,
            gender_id,
            base_section,
            variation_index,
            color_index,
        );
        let start = self.sections.partition_point(|section| section.key() < key);
        let end = self
            .sections
            .partition_point(|section| section.key() <= key);
        &self.sections[start..end]
    }

    /// Finds an exact race/gender/hair-style geometry row.
    #[must_use]
    pub fn hair_geoset(
        &self,
        race_id: u32,
        gender_id: u32,
        variation_id: u32,
    ) -> Option<&CharacterHairGeoset> {
        let key = (race_id, gender_id, variation_id);
        self.hair_geosets
            .binary_search_by_key(&key, |row| row.key())
            .ok()
            .map(|index| &self.hair_geosets[index])
    }

    /// Finds an exact race/gender/facial-hair geometry row.
    #[must_use]
    pub fn facial_hair_style(
        &self,
        race_id: u32,
        gender_id: u32,
        variation_id: u32,
    ) -> Option<&CharacterFacialHairStyle> {
        let key = (race_id, gender_id, variation_id);
        self.facial_hair_styles
            .binary_search_by_key(&key, |row| row.key())
            .ok()
            .map(|index| &self.facial_hair_styles[index])
    }
}

/// Loads one named WDBC table without adding an alternate search path.
fn load_table(store: &mut AssetStore, path: &str) -> Result<WdbcTable, AssetError> {
    WdbcTable::load(store, &AssetPath::new(path)?)
}

/// Decodes the ten-word section layout and sorts by its visible lookup key.
fn decode_sections(table: &WdbcTable) -> Result<Vec<CharacterSection>, AssetError> {
    require_layout(table, 10, "CharSections.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<10>(table, row)?;
        records.push(CharacterSection {
            id: values[0],
            race_id: values[1],
            gender_id: values[2],
            base_section: values[3],
            texture_names: [
                table_string(table, row, 4, values[4])?,
                table_string(table, row, 5, values[5])?,
                table_string(table, row, 6, values[6])?,
            ],
            flags: values[7],
            variation_index: values[8],
            color_index: values[9],
        });
    }
    records.sort_unstable_by_key(|section| (section.key(), section.flags, section.id));
    reject_duplicate_ids(table, &records, CharacterSection::id)?;
    Ok(records)
}

/// Decodes the six-word hair-geoset layout and indexes its compound key.
fn decode_hair_geosets(table: &WdbcTable) -> Result<Vec<CharacterHairGeoset>, AssetError> {
    require_layout(table, 6, "CharHairGeosets.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<6>(table, row)?;
        records.push(CharacterHairGeoset {
            id: values[0],
            race_id: values[1],
            gender_id: values[2],
            variation_id: values[3],
            geoset_id: values[4],
            shows_scalp: values[5],
        });
    }
    records.sort_unstable_by_key(|row| row.key());
    reject_duplicate_keys(table, &records, |row| row.key())?;
    Ok(records)
}

/// Decodes the eight-word facial-hair table, which has no primary-key column.
fn decode_facial_hair(table: &WdbcTable) -> Result<Vec<CharacterFacialHairStyle>, AssetError> {
    require_layout(table, 8, "CharacterFacialHairStyles.dbc")?;
    let mut records = Vec::with_capacity(table.header().record_count() as usize);
    for row in 0..table.header().record_count() {
        let values = read_fields::<8>(table, row)?;
        records.push(CharacterFacialHairStyle {
            race_id: values[0],
            gender_id: values[1],
            variation_id: values[2],
            geosets: values[3..8].try_into().map_err(|_| {
                database_error(table, format!("record {row} facial geosets are truncated"))
            })?,
        });
    }
    records.sort_unstable_by_key(|row| row.key());
    reject_duplicate_keys(table, &records, |row| row.key())?;
    Ok(records)
}

/// Rejects another build's field shape rather than guessing offsets.
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

/// Reads one fixed-width row through the checked raw-table boundary.
fn read_fields<const N: usize>(table: &WdbcTable, row: u32) -> Result<[u32; N], AssetError> {
    let mut values = [0_u32; N];
    for (field, value) in values.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(values)
}

/// Resolves one ASCII asset-name string without constructing a loose-file path.
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

/// Rejects duplicate primary IDs after section-key sorting.
fn reject_duplicate_ids<T>(
    table: &WdbcTable,
    records: &[T],
    id: impl Fn(&T) -> u32,
) -> Result<(), AssetError> {
    let mut ids = records.iter().map(id).collect::<Vec<_>>();
    ids.sort_unstable();
    if let Some(duplicate) = ids.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(database_error(
            table,
            format!("duplicate primary key {}", duplicate[0]),
        ));
    }
    Ok(())
}

/// Rejects ambiguous compound keys used by logarithmic lookup.
fn reject_duplicate_keys<T, K>(
    table: &WdbcTable,
    records: &[T],
    key: impl Fn(&T) -> K,
) -> Result<(), AssetError>
where
    K: Copy + Eq + std::fmt::Debug,
{
    if let Some(duplicate) = records
        .windows(2)
        .find(|pair| key(&pair[0]) == key(&pair[1]))
    {
        return Err(database_error(
            table,
            format!("duplicate lookup key {:?}", key(&duplicate[0])),
        ));
    }
    Ok(())
}

/// Adds the selected archive path to a typed table decode failure.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
