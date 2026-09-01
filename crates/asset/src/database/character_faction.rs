//! Faction-template grouping used to order character-creation races.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::localized_string;
use super::wow_client_db::WdbcTable;

const FACTION_TEMPLATE_PATH: &str = "DBFilesClient\\FactionTemplate.dbc";
const FACTION_GROUP_PATH: &str = "DBFilesClient\\FactionGroup.dbc";

/// One localized faction group returned by stock `GetFactionForRace`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterFactionGroup {
    mask_id: u32,
    internal_name: String,
    name: String,
}

impl CharacterFactionGroup {
    /// Returns the bit index tested against `FactionTemplate.factionGroup`.
    #[must_use]
    pub const fn mask_id(&self) -> u32 {
        self.mask_id
    }

    /// Returns the stable script token, such as `Alliance` or `Horde`.
    #[must_use]
    pub fn internal_name(&self) -> &str {
        &self.internal_name
    }

    /// Returns the selected-locale display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FactionTemplateGroup {
    id: u32,
    group_mask: u32,
}

/// Exact faction-template to faction-group projection used by creation Glue.
pub struct CharacterFactionCatalog {
    templates: Vec<FactionTemplateGroup>,
    groups: Vec<CharacterFactionGroup>,
}

impl CharacterFactionCatalog {
    /// Loads both exact build-12340 faction tables.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for missing tables, mismatched layouts, invalid
    /// strings, duplicate template identifiers, or unusable group mask IDs.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let template_table = load_table(store, FACTION_TEMPLATE_PATH)?;
        require_layout(&template_table, 14, "FactionTemplate.dbc")?;
        let mut templates = Vec::with_capacity(template_table.header().record_count() as usize);
        for row in 0..template_table.header().record_count() {
            templates.push(FactionTemplateGroup {
                id: field(&template_table, row, 0)?,
                group_mask: field(&template_table, row, 3)?,
            });
        }
        templates.sort_unstable_by_key(|template| template.id);
        if let Some(duplicate) = templates.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(database_error(
                &template_table,
                format!("duplicate primary key {}", duplicate[0].id),
            ));
        }

        let group_table = load_table(store, FACTION_GROUP_PATH)?;
        require_layout(&group_table, 20, "FactionGroup.dbc")?;
        let locale = store.locale();
        let mut groups = Vec::with_capacity(group_table.header().record_count() as usize);
        for row in 0..group_table.header().record_count() {
            let mask_id = field(&group_table, row, 1)?;
            if mask_id >= u32::BITS {
                return Err(database_error(
                    &group_table,
                    format!("record {row} mask ID {mask_id} exceeds a 32-bit faction mask"),
                ));
            }
            groups.push(CharacterFactionGroup {
                mask_id,
                internal_name: string(&group_table, row, 2)?,
                name: localized_string(&group_table, row, 3, locale)?,
            });
        }
        Ok(Self { templates, groups })
    }

    /// Resolves the first physical faction group whose bit matches a template.
    #[must_use]
    pub fn group_for_template(&self, faction_template_id: u32) -> Option<&CharacterFactionGroup> {
        let index = self
            .templates
            .binary_search_by_key(&faction_template_id, |template| template.id)
            .ok()?;
        let mask = self.templates[index].group_mask;
        self.groups
            .iter()
            .find(|group| group.mask_id != 0 && mask & (1_u32 << group.mask_id) != 0)
    }
}

fn load_table(store: &mut AssetStore, path: &str) -> Result<WdbcTable, AssetError> {
    WdbcTable::load(store, &AssetPath::new(path)?)
}

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
    if !bytes.is_ascii() {
        return Err(database_error(
            table,
            format!("record {row} field {column} contains non-ASCII internal text"),
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}

fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
