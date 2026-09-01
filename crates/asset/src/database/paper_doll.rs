//! Build-12340 paper-doll inventory slot names, icons, and numeric identities.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const PAPER_DOLL_ITEM_FRAME_PATH: &str = "DBFilesClient\\PaperDollItemFrame.dbc";
const PAPER_DOLL_ITEM_FRAME_FIELD_COUNT: u32 = 3;

/// One exact `PaperDollItemFrame.dbc` inventory-slot row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperDollItemFrameDefinition {
    item_button_name: String,
    slot_icon: String,
    slot_number: u32,
}

impl PaperDollItemFrameDefinition {
    /// Returns the case-insensitive script name accepted by the native API.
    #[must_use]
    pub fn item_button_name(&self) -> &str {
        &self.item_button_name
    }

    /// Returns the archive texture used for an empty paper-doll slot.
    #[must_use]
    pub fn slot_icon(&self) -> &str {
        &self.slot_icon
    }

    /// Returns the numeric inventory slot consumed by unit-field APIs.
    #[must_use]
    pub const fn slot_number(&self) -> u32 {
        self.slot_number
    }
}

/// Client-authored inventory-slot catalog used by `GetInventorySlotInfo`.
pub struct PaperDollItemFrameCatalog {
    definitions: Vec<PaperDollItemFrameDefinition>,
}

impl PaperDollItemFrameCatalog {
    /// Loads the exact three-field table through normal archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, uses a
    /// different layout, contains invalid text, or repeats a script name.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new(PAPER_DOLL_ITEM_FRAME_PATH)?)?;
        require_layout(&table)?;
        let mut definitions = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            definitions.push(PaperDollItemFrameDefinition {
                item_button_name: string_field(&table, row, 0)?,
                slot_icon: string_field(&table, row, 1)?,
                slot_number: field(&table, row, 2)?,
            });
        }
        definitions.sort_by_key(|definition| definition.item_button_name.to_ascii_lowercase());
        if let Some(duplicate) = definitions.windows(2).find(|pair| {
            pair[0]
                .item_button_name
                .eq_ignore_ascii_case(&pair[1].item_button_name)
        }) {
            return Err(database_error(
                &table,
                format!(
                    "duplicate item button name {}",
                    duplicate[0].item_button_name
                ),
            ));
        }
        Ok(Self { definitions })
    }

    /// Returns all authored rows in case-insensitive script-name order.
    pub fn definitions(&self) -> impl ExactSizeIterator<Item = &PaperDollItemFrameDefinition> {
        self.definitions.iter()
    }

    /// Finds one case-insensitive script slot without substituting another row.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&PaperDollItemFrameDefinition> {
        self.definitions
            .binary_search_by(|definition| {
                definition
                    .item_button_name
                    .to_ascii_lowercase()
                    .cmp(&name.to_ascii_lowercase())
            })
            .ok()
            .map(|index| &self.definitions[index])
    }
}

fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == PAPER_DOLL_ITEM_FRAME_FIELD_COUNT
        && header.record_size() == PAPER_DOLL_ITEM_FRAME_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 PaperDollItemFrame.dbc requires 3 fields and 12-byte records; found {} fields and {}-byte records",
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

fn string_field(table: &WdbcTable, row: u32, column: u32) -> Result<String, AssetError> {
    let offset = field(table, row, column)?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} field {column} has invalid string offset {offset}"),
        )
    })?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| {
            database_error(
                table,
                format!("record {row} field {column} is not UTF-8: {error}"),
            )
        })
}
