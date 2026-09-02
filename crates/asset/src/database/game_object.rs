//! Build-12340 game-object display identities and authored bounds.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const GAME_OBJECT_DISPLAY_INFO_PATH: &str = "DBFilesClient\\GameObjectDisplayInfo.dbc";
const GAME_OBJECT_DISPLAY_INFO_FIELD_COUNT: u32 = 19;

/// One exact `GameObjectDisplayInfo.dbc` presentation record.
#[derive(Clone, Debug, PartialEq)]
pub struct GameObjectDisplayInfo {
    id: u32,
    asset_path: AssetPath,
    minimum: [f32; 3],
    maximum: [f32; 3],
}

impl GameObjectDisplayInfo {
    /// Returns the identifier referenced by `GAMEOBJECT_DISPLAYID`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the canonical archive identity of the M2 or WMO resource.
    #[must_use]
    pub const fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    /// Returns the authored model-space minimum corner.
    #[must_use]
    pub const fn minimum(&self) -> [f32; 3] {
        self.minimum
    }

    /// Returns the authored model-space maximum corner.
    #[must_use]
    pub const fn maximum(&self) -> [f32; 3] {
        self.maximum
    }
}

/// Identifier-indexed build-12340 game-object display records.
pub struct GameObjectDisplayCatalog {
    displays: Vec<GameObjectDisplayInfo>,
}

impl GameObjectDisplayCatalog {
    /// Loads the exact 19-word `GameObjectDisplayInfo.dbc` layout.
    ///
    /// Empty asset names are non-rendering stock records and are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for storage, layout, path, bounds, or duplicate
    /// identifier failures.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(GAME_OBJECT_DISPLAY_INFO_PATH)?)?;
        require_layout(&table)?;
        let mut displays = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let id = field(&table, row, 0)?;
            if id == 0 {
                continue;
            }
            let path = string(&table, row, 1)?;
            if path.is_empty() {
                continue;
            }
            let minimum = float_vector(&table, row, 12)?;
            let maximum = float_vector(&table, row, 15)?;
            displays.push(GameObjectDisplayInfo {
                id,
                asset_path: AssetPath::new(path)?,
                minimum,
                maximum,
            });
        }
        displays.sort_unstable_by_key(GameObjectDisplayInfo::id);
        if let Some(duplicate) = displays.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { displays })
    }

    /// Finds one exact client-authored display identifier.
    #[must_use]
    pub fn display(&self, id: u32) -> Option<&GameObjectDisplayInfo> {
        self.displays
            .binary_search_by_key(&id, GameObjectDisplayInfo::id)
            .ok()
            .map(|index| &self.displays[index])
    }

    /// Returns all rendering records in ascending identifier order.
    #[must_use]
    pub fn displays(&self) -> &[GameObjectDisplayInfo] {
        &self.displays
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == GAME_OBJECT_DISPLAY_INFO_FIELD_COUNT
        && header.record_size() == GAME_OBJECT_DISPLAY_INFO_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 GameObjectDisplayInfo.dbc requires 19 fields and 76-byte records; found {} fields and {}-byte records",
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

fn float_vector(table: &WdbcTable, row: u32, first_column: u32) -> Result<[f32; 3], AssetError> {
    let values = [
        f32::from_bits(field(table, row, first_column)?),
        f32::from_bits(field(table, row, first_column + 1)?),
        f32::from_bits(field(table, row, first_column + 2)?),
    ];
    if values.into_iter().all(f32::is_finite) {
        return Ok(values);
    }
    Err(database_error(
        table,
        format!("record {row} has nonfinite game-object display bounds"),
    ))
}
