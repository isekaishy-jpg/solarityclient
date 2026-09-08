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
/// Complete faction-template inputs to the native directed reaction query.
pub struct FactionTemplateDefinition {
    id: u32,
    faction_id: u32,
    flags: u32,
    group_mask: u32,
    friend_mask: u32,
    enemy_mask: u32,
    enemies: [u32; 4],
    friends: [u32; 4],
}

impl FactionTemplateDefinition {
    /// Creates the complete fourteen-word build-12340 faction template.
    #[must_use]
    pub const fn from_words(words: [u32; 14]) -> Self {
        Self {
            id: words[0],
            faction_id: words[1],
            flags: words[2],
            group_mask: words[3],
            friend_mask: words[4],
            enemy_mask: words[5],
            enemies: [words[6], words[7], words[8], words[9]],
            friends: [words[10], words[11], words[12], words[13]],
        }
    }

    /// Returns the primary template key.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }
    /// Returns the referenced Faction.dbc key.
    #[must_use]
    pub const fn faction_id(self) -> u32 {
        self.faction_id
    }
    /// Returns the native faction policy flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }
    /// Returns this template's group membership mask.
    #[must_use]
    pub const fn group_mask(self) -> u32 {
        self.group_mask
    }
    /// Returns the friendly group mask.
    #[must_use]
    pub const fn friend_mask(self) -> u32 {
        self.friend_mask
    }
    /// Returns the hostile group mask.
    #[must_use]
    pub const fn enemy_mask(self) -> u32 {
        self.enemy_mask
    }
    /// Returns ordered hostile faction keys, terminated by the first zero.
    #[must_use]
    pub const fn enemies(&self) -> &[u32; 4] {
        &self.enemies
    }
    /// Returns ordered friendly faction keys, terminated by the first zero.
    #[must_use]
    pub const fn friends(&self) -> &[u32; 4] {
        &self.friends
    }
}

/// Exact faction-template to faction-group projection used by creation Glue.
pub struct CharacterFactionCatalog {
    templates: Vec<FactionTemplateDefinition>,
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
            let mut words = [0; 14];
            for (index, word) in words.iter_mut().enumerate() {
                *word = field(&template_table, row, index as u32)?;
            }
            templates.push(FactionTemplateDefinition::from_words(words));
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

    /// Resolves the complete template used by native unit-reaction queries.
    #[must_use]
    pub fn template(&self, id: u32) -> Option<&FactionTemplateDefinition> {
        self.templates
            .binary_search_by_key(&id, |template| template.id)
            .ok()
            .map(|index| &self.templates[index])
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
