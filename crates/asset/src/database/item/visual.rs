//! Exact item-visual and enchantment visual joins for build 12340.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::wow_client_db::WdbcTable;

const ITEM_VISUALS_PATH: &str = "DBFilesClient\\ItemVisuals.dbc";
const ITEM_VISUAL_EFFECTS_PATH: &str = "DBFilesClient\\ItemVisualEffects.dbc";
const SPELL_ITEM_ENCHANTMENT_PATH: &str = "DBFilesClient\\SpellItemEnchantment.dbc";
const ITEM_VISUAL_FIELD_COUNT: u32 = 6;
const ITEM_VISUAL_EFFECT_FIELD_COUNT: u32 = 2;
const SPELL_ITEM_ENCHANTMENT_FIELD_COUNT: u32 = 38;

/// One five-slot item visual definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemVisual {
    id: u32,
    effect_ids: [u32; 5],
}

impl ItemVisual {
    /// Returns the primary key referenced by display and enchantment rows.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns effect identifiers in item-M2 attachment order `0..4`.
    #[must_use]
    pub const fn effect_ids(&self) -> [u32; 5] {
        self.effect_ids
    }
}

/// One optional M2 path used by an item-visual slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemVisualEffect {
    id: u32,
    model_path: Option<AssetPath>,
}

impl ItemVisualEffect {
    /// Returns the identifier stored in an [`ItemVisual`] slot.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored effect M2 path, preserving an empty row as absent.
    #[must_use]
    pub const fn model_path(&self) -> Option<&AssetPath> {
        self.model_path.as_ref()
    }
}

/// The client-visible portion of one spell item enchantment row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpellItemEnchantment {
    id: u32,
    item_visual_id: u32,
}

impl SpellItemEnchantment {
    /// Returns the enchantment identifier carried by public player fields.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the referenced `ItemVisuals.dbc` identifier.
    #[must_use]
    pub const fn item_visual_id(&self) -> u32 {
        self.item_visual_id
    }
}

/// Identifier-indexed item visual, effect, and enchantment tables.
pub struct ItemVisualCatalog {
    visuals: Vec<ItemVisual>,
    effects: Vec<ItemVisualEffect>,
    enchantments: Vec<SpellItemEnchantment>,
}

impl ItemVisualCatalog {
    /// Loads all three tables through ordinary archive precedence.
    ///
    /// Dangling effect identifiers are retained because stock's own table has
    /// unused slots containing sentinel and stale values. Consumers resolve
    /// each slot independently and omit only identifiers with no effect row.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when a table is absent, malformed, belongs to a
    /// different schema, contains an invalid model path, or repeats a key.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let visual_table = load_table(store, ITEM_VISUALS_PATH)?;
        let effect_table = load_table(store, ITEM_VISUAL_EFFECTS_PATH)?;
        let enchantment_table = load_table(store, SPELL_ITEM_ENCHANTMENT_PATH)?;
        require_layout(&visual_table, ITEM_VISUAL_FIELD_COUNT, "ItemVisuals")?;
        require_layout(
            &effect_table,
            ITEM_VISUAL_EFFECT_FIELD_COUNT,
            "ItemVisualEffects",
        )?;
        require_layout(
            &enchantment_table,
            SPELL_ITEM_ENCHANTMENT_FIELD_COUNT,
            "SpellItemEnchantment",
        )?;

        let mut visuals = Vec::with_capacity(visual_table.header().record_count() as usize);
        for row in 0..visual_table.header().record_count() {
            visuals.push(ItemVisual {
                id: field(&visual_table, row, 0)?,
                effect_ids: [
                    field(&visual_table, row, 1)?,
                    field(&visual_table, row, 2)?,
                    field(&visual_table, row, 3)?,
                    field(&visual_table, row, 4)?,
                    field(&visual_table, row, 5)?,
                ],
            });
        }
        sort_unique(&visual_table, &mut visuals, ItemVisual::id)?;

        let mut effects = Vec::with_capacity(effect_table.header().record_count() as usize);
        for row in 0..effect_table.header().record_count() {
            let id = field(&effect_table, row, 0)?;
            let offset = field(&effect_table, row, 1)?;
            let bytes = effect_table.string_bytes(offset).ok_or_else(|| {
                database_error(
                    &effect_table,
                    format!("record {row} has invalid model-path offset {offset}"),
                )
            })?;
            if !bytes.is_ascii() || bytes.contains(&0) {
                return Err(database_error(
                    &effect_table,
                    format!("record {row} contains an invalid model path"),
                ));
            }
            let value = String::from_utf8(bytes.to_vec())
                .map_err(|error| database_error(&effect_table, error.to_string()))?;
            let model_path = if value.ends_with('\\') || value.is_empty() {
                None
            } else {
                Some(AssetPath::new(value)?)
            };
            effects.push(ItemVisualEffect { id, model_path });
        }
        sort_unique(&effect_table, &mut effects, ItemVisualEffect::id)?;

        let mut enchantments =
            Vec::with_capacity(enchantment_table.header().record_count() as usize);
        for row in 0..enchantment_table.header().record_count() {
            enchantments.push(SpellItemEnchantment {
                id: field(&enchantment_table, row, 0)?,
                item_visual_id: field(&enchantment_table, row, 31)?,
            });
        }
        sort_unique(
            &enchantment_table,
            &mut enchantments,
            SpellItemEnchantment::id,
        )?;

        Ok(Self {
            visuals,
            effects,
            enchantments,
        })
    }

    /// Finds an exact five-slot visual definition.
    #[must_use]
    pub fn visual(&self, id: u32) -> Option<ItemVisual> {
        self.visuals
            .binary_search_by_key(&id, |visual| visual.id)
            .ok()
            .map(|index| self.visuals[index])
    }

    /// Finds an exact effect row without substituting another slot.
    #[must_use]
    pub fn effect(&self, id: u32) -> Option<&ItemVisualEffect> {
        self.effects
            .binary_search_by_key(&id, |effect| effect.id)
            .ok()
            .map(|index| &self.effects[index])
    }

    /// Finds an exact public enchantment identifier.
    #[must_use]
    pub fn enchantment(&self, id: u32) -> Option<SpellItemEnchantment> {
        self.enchantments
            .binary_search_by_key(&id, |enchantment| enchantment.id)
            .ok()
            .map(|index| self.enchantments[index])
    }
}

/// Loads one WDBC path through the shared arbitrary-file boundary.
fn load_table(store: &mut AssetStore, path: &str) -> Result<WdbcTable, AssetError> {
    WdbcTable::load(store, &AssetPath::new(path)?)
}

/// Rejects tables whose field stride is not build 12340's exact layout.
fn require_layout(table: &WdbcTable, fields: u32, name: &str) -> Result<(), AssetError> {
    let header = table.header();
    if header.field_count() == fields && header.record_size() == fields * 4 {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 {name}.dbc requires {fields} fields and {}-byte records; found {} fields and {}-byte records",
            fields * 4,
            header.field_count(),
            header.record_size()
        ),
    ))
}

/// Reads one checked unsigned WDBC field.
fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("record {row} field {column} is truncated")))
}

/// Sorts one table by primary key and rejects duplicate identifiers.
fn sort_unique<T, F>(table: &WdbcTable, rows: &mut [T], key: F) -> Result<(), AssetError>
where
    F: Fn(&T) -> u32,
{
    rows.sort_unstable_by_key(&key);
    if let Some(pair) = rows.windows(2).find(|pair| key(&pair[0]) == key(&pair[1])) {
        return Err(database_error(
            table,
            format!("duplicate primary key {}", key(&pair[0])),
        ));
    }
    Ok(())
}

/// Adds the selected table path to a stable database decode error.
fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
