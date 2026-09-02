//! Build-12340 loading-card identities and archive texture paths.

use crate::{AssetError, AssetPath, AssetStore};

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const LOADING_SCREENS_PATH: &str = "DBFilesClient\\LoadingScreens.dbc";
const FIELD_COUNT: u32 = 4;

/// One client-authored loading-card record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadingScreenDefinition {
    id: u32,
    name: String,
    texture: AssetPath,
    has_widescreen: bool,
}

impl LoadingScreenDefinition {
    /// Returns the identifier referenced by `Map.dbc`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the client-authored diagnostic name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the ordinary 4:3 loading-card texture path.
    #[must_use]
    pub const fn texture(&self) -> &AssetPath {
        &self.texture
    }

    /// Reports whether stock authored a `Wide` texture variant.
    #[must_use]
    pub const fn has_widescreen(&self) -> bool {
        self.has_widescreen
    }

    /// Returns the stock widescreen archive identity when this record authors one.
    ///
    /// Build 12340 inserts `Wide` immediately before the canonical texture
    /// extension. Presence remains an archive-stack decision because stock
    /// falls back to the narrow card when the derived asset is absent.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] if the derived archive path is invalid.
    pub fn widescreen_texture(&self) -> Result<Option<AssetPath>, AssetError> {
        if !self.has_widescreen {
            return Ok(None);
        }
        let value = self.texture.as_str();
        let extension = value.rfind('.').unwrap_or(value.len());
        let mut wide = String::with_capacity(value.len() + 4);
        wide.push_str(&value[..extension]);
        wide.push_str("Wide");
        wide.push_str(&value[extension..]);
        AssetPath::new(wide).map(Some)
    }
}

/// Identifier-indexed build-12340 loading-card definitions.
pub struct LoadingScreenCatalog {
    screens: Vec<LoadingScreenDefinition>,
}

impl LoadingScreenCatalog {
    /// Loads and validates the exact four-word `LoadingScreens.dbc` layout.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(LOADING_SCREENS_PATH)?)?;
        require_layout(&table)?;
        let mut screens = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            screens.push(LoadingScreenDefinition {
                id: field(&table, row, 0)?,
                name: string(&table, row, 1)?,
                texture: texture_path(&table, row, 2)?,
                has_widescreen: field(&table, row, 3)? != 0,
            });
        }
        screens.sort_unstable_by_key(LoadingScreenDefinition::id);
        if let Some(duplicate) = screens.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { screens })
    }

    /// Finds one exact client loading-screen identifier.
    #[must_use]
    pub fn screen(&self, id: u32) -> Option<&LoadingScreenDefinition> {
        self.screens
            .binary_search_by_key(&id, LoadingScreenDefinition::id)
            .ok()
            .map(|index| &self.screens[index])
    }

    /// Returns all definitions in ascending identifier order.
    #[must_use]
    pub fn screens(&self) -> &[LoadingScreenDefinition] {
        &self.screens
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == FIELD_COUNT && header.record_size() == FIELD_COUNT * 4 {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 LoadingScreens.dbc requires 4 fields and 16-byte records; found {} fields and {}-byte records",
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

/// Applies the same build-12340 texture-name rules used by FrameXML. Stock
/// tables may omit the extension or retain the source-art `.tga` suffix even
/// though the archive payload is a BLP.
fn texture_path(table: &WdbcTable, row: u32, column: u32) -> Result<AssetPath, AssetError> {
    let value = string(table, row, column)?;
    let lower = value.to_ascii_lowercase();
    let canonical = if lower.ends_with(".tga") {
        format!("{}.blp", &value[..value.len() - 4])
    } else if lower.ends_with(".blp") {
        value
    } else if value
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|component| !component.contains('.'))
    {
        format!("{value}.blp")
    } else {
        return Err(database_error(
            table,
            format!("record {row} field {column} has unsupported texture extension in {value}"),
        ));
    };
    AssetPath::new(canonical)
}
