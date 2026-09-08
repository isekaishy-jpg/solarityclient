//! Build-12340 map identities and exact client terrain directory names.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::{database_error, localized_string};
use super::wow_client_db::WdbcTable;

const MAP_TABLE_PATH: &str = "DBFilesClient\\Map.dbc";
const MAP_TABLE_FIELD_COUNT: u32 = 66;
const LOCALIZED_NAME_FIRST_FIELD: u32 = 5;

/// Stock's `INSTANCE_TYPE` values carried by `Map.dbc`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapKind {
    /// An outdoor continent or ordinary world map.
    World,
    /// A non-raid instance.
    Dungeon,
    /// A raid instance.
    Raid,
    /// A battleground.
    Battleground,
    /// An arena.
    Arena,
}

impl MapKind {
    fn decode(table: &WdbcTable, row: u32, value: u32) -> Result<Self, AssetError> {
        match value {
            0 => Ok(Self::World),
            1 => Ok(Self::Dungeon),
            2 => Ok(Self::Raid),
            3 => Ok(Self::Battleground),
            4 => Ok(Self::Arena),
            _ => Err(database_error(
                table,
                format!("record {row} has unknown instance type {value}"),
            )),
        }
    }
}

/// One client-authored map definition used to resolve world assets.
#[derive(Clone, Debug, PartialEq)]
pub struct MapDefinition {
    id: u32,
    directory: String,
    kind: MapKind,
    flags: u32,
    name: String,
    linked_zone_id: u32,
    loading_screen_id: u32,
    entrance_map_id: i32,
    entrance: [f32; 2],
    expansion_id: u32,
    maximum_players: u32,
    time_of_day_override: i32,
}

impl MapDefinition {
    /// Returns the native minute-of-day override, or `None` for realm time.
    #[must_use]
    pub const fn time_of_day_override(&self) -> Option<i32> {
        if self.time_of_day_override == -1 {
            None
        } else {
            Some(self.time_of_day_override)
        }
    }
    /// Returns the identifier used by world-login and transfer packets.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the exact `World\\Maps` directory stem authored by the client.
    #[must_use]
    pub fn directory(&self) -> &str {
        &self.directory
    }

    /// Returns the stock map instance classification.
    #[must_use]
    pub const fn kind(&self) -> MapKind {
        self.kind
    }

    /// Returns the unmodified build-12340 map flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the exact selected-locale display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the common-zone identifier used by instances and continents.
    #[must_use]
    pub const fn linked_zone_id(&self) -> u32 {
        self.linked_zone_id
    }

    /// Returns the `LoadingScreens.dbc` identifier selected for this map.
    #[must_use]
    pub const fn loading_screen_id(&self) -> u32 {
        self.loading_screen_id
    }

    /// Returns the authored entrance map and coordinates when one exists.
    #[must_use]
    pub const fn entrance(&self) -> Option<(i32, [f32; 2])> {
        if self.entrance_map_id < 0 {
            None
        } else {
            Some((self.entrance_map_id, self.entrance))
        }
    }

    /// Returns the client expansion identifier.
    #[must_use]
    pub const fn expansion_id(&self) -> u32 {
        self.expansion_id
    }

    /// Returns the map's fallback maximum-player count.
    #[must_use]
    pub const fn maximum_players(&self) -> u32 {
        self.maximum_players
    }
}

/// Identifier-indexed build-12340 map definitions.
pub struct MapCatalog {
    maps: Vec<MapDefinition>,
}

impl MapCatalog {
    /// Loads the exact 66-word `Map.dbc` layout.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, layout, text, enum, or duplicate-key
    /// failures. Directory strings are validated as archive path components.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let locale = store.locale();
        let table = WdbcTable::load(store, &AssetPath::new(MAP_TABLE_PATH)?)?;
        require_layout(&table)?;
        let mut maps = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let directory = string(&table, row, 1)?;
            validate_directory(&table, row, &directory)?;
            maps.push(MapDefinition {
                id: field(&table, row, 0)?,
                directory,
                kind: MapKind::decode(&table, row, field(&table, row, 2)?)?,
                flags: field(&table, row, 3)?,
                name: localized_string(&table, row, LOCALIZED_NAME_FIRST_FIELD, locale)?,
                linked_zone_id: field(&table, row, 22)?,
                loading_screen_id: field(&table, row, 57)?,
                entrance_map_id: field(&table, row, 59)? as i32,
                entrance: [
                    f32::from_bits(field(&table, row, 60)?),
                    f32::from_bits(field(&table, row, 61)?),
                ],
                expansion_id: field(&table, row, 63)?,
                maximum_players: field(&table, row, 65)?,
                time_of_day_override: field(&table, row, 62)? as i32,
            });
        }
        maps.sort_unstable_by_key(MapDefinition::id);
        if let Some(duplicate) = maps.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { maps })
    }

    /// Finds one exact client map identifier.
    #[must_use]
    pub fn map(&self, id: u32) -> Option<&MapDefinition> {
        self.maps
            .binary_search_by_key(&id, MapDefinition::id)
            .ok()
            .map(|index| &self.maps[index])
    }

    /// Returns all definitions in ascending identifier order.
    #[must_use]
    pub fn maps(&self) -> &[MapDefinition] {
        &self.maps
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == MAP_TABLE_FIELD_COUNT
        && header.record_size() == MAP_TABLE_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 Map.dbc requires 66 fields and 264-byte records; found {} fields and {}-byte records",
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
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}

fn validate_directory(table: &WdbcTable, row: u32, directory: &str) -> Result<(), AssetError> {
    if directory.is_empty() || !directory.is_ascii() || directory.contains(['\\', '/', '\0']) {
        return Err(database_error(
            table,
            format!("record {row} has invalid map directory {directory:?}"),
        ));
    }
    Ok(())
}
