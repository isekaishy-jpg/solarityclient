//! Exact `ItemDisplayInfo.dbc` decoding and identifier lookup.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const ITEM_DISPLAY_INFO_PATH: &str = "DBFilesClient\\ItemDisplayInfo.dbc";
const ITEM_DISPLAY_FIELD_COUNT: u32 = 25;

/// One exact build-12340 item display row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemDisplayInfo {
    id: u32,
    model_names: [String; 2],
    model_textures: [String; 2],
    inventory_icons: [String; 2],
    geoset_groups: [u32; 3],
    flags: u32,
    spell_visual_id: u32,
    group_sound_index: u32,
    helmet_geoset_visibility_ids: [u32; 2],
    component_textures: [String; 8],
    item_visual_id: u32,
    particle_color_id: u32,
}

impl ItemDisplayInfo {
    /// Returns the display identifier carried by visible-equipment fields.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the left/right model filename stems in stock order.
    #[must_use]
    pub fn model_names(&self) -> [&str; 2] {
        self.model_names.each_ref().map(String::as_str)
    }

    /// Returns the left/right model texture stems in stock order.
    #[must_use]
    pub fn model_textures(&self) -> [&str; 2] {
        self.model_textures.each_ref().map(String::as_str)
    }

    /// Returns both inventory icon names, preserving authored absence.
    #[must_use]
    pub fn inventory_icons(&self) -> [&str; 2] {
        self.inventory_icons.each_ref().map(String::as_str)
    }

    /// Returns the three equipment geoset selectors.
    #[must_use]
    pub const fn geoset_groups(&self) -> [u32; 3] {
        self.geoset_groups
    }

    /// Returns the exact item-display flags word.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the attached spell visual identifier.
    #[must_use]
    pub const fn spell_visual_id(&self) -> u32 {
        self.spell_visual_id
    }

    /// Returns the item group-sound index.
    #[must_use]
    pub const fn group_sound_index(&self) -> u32 {
        self.group_sound_index
    }

    /// Returns male/female helmet geoset-visibility row identifiers.
    #[must_use]
    pub const fn helmet_geoset_visibility_ids(&self) -> [u32; 2] {
        self.helmet_geoset_visibility_ids
    }

    /// Returns the eight body-region texture stems in stock section order.
    #[must_use]
    pub fn component_textures(&self) -> [&str; 8] {
        self.component_textures.each_ref().map(String::as_str)
    }

    /// Returns the item visual identifier used by attached models.
    #[must_use]
    pub const fn item_visual_id(&self) -> u32 {
        self.item_visual_id
    }

    /// Returns the particle color row identifier.
    #[must_use]
    pub const fn particle_color_id(&self) -> u32 {
        self.particle_color_id
    }
}

/// Identifier-indexed build-12340 item display records.
pub struct ItemDisplayCatalog {
    displays: Vec<ItemDisplayInfo>,
}

impl ItemDisplayCatalog {
    /// Loads the exact stock table through ordinary archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the table is absent, malformed, has another
    /// build's field layout, contains invalid strings, or repeats an identifier.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(ITEM_DISPLAY_INFO_PATH)?;
        let table = WdbcTable::load(store, &path)?;
        require_layout(&table)?;

        let mut displays = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let fields = read_fields::<25>(&table, row)?;
            let component_offsets = fields[15..23].try_into().map_err(|_| {
                database_error(
                    &table,
                    format!("record {row} component textures are truncated"),
                )
            })?;
            displays.push(ItemDisplayInfo {
                id: fields[0],
                model_names: read_strings(&table, row, 1, [fields[1], fields[2]])?,
                model_textures: read_strings(&table, row, 3, [fields[3], fields[4]])?,
                inventory_icons: read_strings(&table, row, 5, [fields[5], fields[6]])?,
                geoset_groups: [fields[7], fields[8], fields[9]],
                flags: fields[10],
                spell_visual_id: fields[11],
                group_sound_index: fields[12],
                helmet_geoset_visibility_ids: [fields[13], fields[14]],
                component_textures: read_strings(&table, row, 15, component_offsets)?,
                item_visual_id: fields[23],
                particle_color_id: fields[24],
            });
        }
        displays.sort_unstable_by_key(ItemDisplayInfo::id);
        if let Some(duplicate) = displays.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }
        Ok(Self { displays })
    }

    /// Finds an exact item display identifier without substituting another row.
    #[must_use]
    pub fn display(&self, id: u32) -> Option<&ItemDisplayInfo> {
        self.displays
            .binary_search_by_key(&id, ItemDisplayInfo::id)
            .ok()
            .map(|index| &self.displays[index])
    }
}

/// Rejects a table from another client build before reading field offsets.
fn require_layout(table: &WdbcTable) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == ITEM_DISPLAY_FIELD_COUNT
        && header.record_size() == ITEM_DISPLAY_FIELD_COUNT * 4
    {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 ItemDisplayInfo.dbc requires 25 fields and 100-byte records; found {} fields and {}-byte records",
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one fixed-width item display row through checked table access.
fn read_fields<const N: usize>(table: &WdbcTable, row: u32) -> Result<[u32; N], AssetError> {
    let mut fields = [0_u32; N];
    for (field, value) in fields.iter_mut().enumerate() {
        *value = table.field_u32(row, field as u32).ok_or_else(|| {
            database_error(table, format!("record {row} field {field} is truncated"))
        })?;
    }
    Ok(fields)
}

/// Reads an adjacent fixed-size group of ASCII DBC strings.
fn read_strings<const N: usize>(
    table: &WdbcTable,
    row: u32,
    first_field: u32,
    offsets: [u32; N],
) -> Result<[String; N], AssetError> {
    let mut strings = Vec::with_capacity(N);
    for (index, offset) in offsets.into_iter().enumerate() {
        let field = first_field + index as u32;
        let bytes = table.string_bytes(offset).ok_or_else(|| {
            database_error(
                table,
                format!("record {row} field {field} has invalid string offset {offset}"),
            )
        })?;
        if !bytes.is_ascii() || bytes.contains(&0) {
            return Err(database_error(
                table,
                format!("record {row} field {field} contains an invalid asset stem"),
            ));
        }
        strings.push(
            String::from_utf8(bytes.to_vec())
                .map_err(|error| database_error(table, error.to_string()))?,
        );
    }
    strings.try_into().map_err(|_values: Vec<String>| {
        database_error(table, format!("record {row} string group is truncated"))
    })
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
