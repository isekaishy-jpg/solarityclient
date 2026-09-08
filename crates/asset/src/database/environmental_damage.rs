//! EnvironmentalDamage.dbc and the SpellVisualKit rows it selects.

use super::{WdbcTable, localized::database_error};
use crate::{AssetError, AssetPath, AssetStore};
use std::collections::BTreeMap;

/// The complete thirty-eight-word declaration selected for an environmental hit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentalVisualKit {
    words: [u32; 38],
}

impl EnvironmentalVisualKit {
    /// Returns the native SpellVisualKit key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.words[0]
    }
    /// Returns all source words without losing float bits or sentinel values.
    #[must_use]
    pub const fn words(&self) -> &[u32; 38] {
        &self.words
    }
    /// Returns the animation selected by the environmental impact phase.
    #[must_use]
    pub const fn animation(&self) -> i32 {
        self.words[2] as i32
    }
    /// Returns the authored SoundEntries identifier, including zero for none.
    #[must_use]
    pub const fn sound_entry_id(&self) -> u32 {
        self.words[15]
    }
    /// Returns effects in 745230's environmental phase order with attachment IDs.
    /// Attachment -1 is the standalone world effect channel.
    pub fn effects(&self) -> impl Iterator<Item = (i32, u32)> + '_ {
        [
            (3, 20),
            (14, -1),
            (5, 19),
            (6, 21),
            (7, 22),
            (8, 17),
            (4, 34),
            (9, 2),
            (10, 1),
            (11, 23),
            (12, 24),
            (13, 25),
        ]
        .into_iter()
        .filter_map(|(field, attachment)| {
            (self.words[field] != 0).then_some((attachment, self.words[field]))
        })
    }
    /// Returns the four special-effect slots and their four float parameters.
    pub fn special_effects(&self) -> impl Iterator<Item = (i32, [f32; 4])> + '_ {
        (0..4).filter_map(|slot| {
            let kind = self.words[17 + slot] as i32;
            (kind >= 0).then(|| {
                (
                    kind,
                    std::array::from_fn(|parameter| {
                        f32::from_bits(self.words[21 + slot + parameter * 4])
                    }),
                )
            })
        })
    }
}

/// Six native category slots, resolved through the installed visual-kit table.
#[derive(Default)]
pub struct EnvironmentalDamageCatalog {
    slots: [u32; 6],
    kits: BTreeMap<u32, EnvironmentalVisualKit>,
}

impl EnvironmentalDamageCatalog {
    /// Loads declarations using native reverse physical-row category assignment.
    ///
    /// # Errors
    /// Returns an asset error for missing tables, wrong schemas or duplicate kit keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let mapping = WdbcTable::load(
            store,
            &AssetPath::new("DBFilesClient/EnvironmentalDamage.dbc")?,
        )?;
        require_layout(&mapping, 3)?;
        let mut slots = [0; 6];
        for row in (0..mapping.header().record_count()).rev() {
            let kind = field(&mapping, row, 1)?;
            if let Some(slot) = slots.get_mut(kind as usize) {
                *slot = field(&mapping, row, 2)?;
            }
        }
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient/SpellVisualKit.dbc")?)?;
        require_layout(&table, 38)?;
        let mut kits = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let id = field(&table, row, 0)?;
            if id == 0 || !slots.contains(&id) {
                continue;
            }
            let mut words = [0; 38];
            for (column, value) in words.iter_mut().enumerate() {
                *value = field(&table, row, column as u32)?;
            }
            if kits.insert(id, EnvironmentalVisualKit { words }).is_some() {
                return Err(database_error(
                    &table,
                    format!("duplicate primary key {id}"),
                ));
            }
        }
        Ok(Self { slots, kits })
    }

    /// Returns the exact native category's declaration, if both table rows exist.
    #[must_use]
    pub fn visual_kit(&self, kind: u8) -> Option<&EnvironmentalVisualKit> {
        self.kits.get(self.slots.get(usize::from(kind))?)
    }
    /// Returns each selected declaration once, in primary-key order.
    pub fn visual_kits(&self) -> impl Iterator<Item = &EnvironmentalVisualKit> {
        self.kits.values()
    }
}

fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| database_error(table, format!("truncated row {row}, field {column}")))
}
fn require_layout(table: &WdbcTable, fields: u32) -> Result<(), AssetError> {
    if table.header().field_count() == fields && table.header().record_size() == fields * 4 {
        Ok(())
    } else {
        Err(database_error(
            table,
            format!(
                "build 12340 requires {fields} fields and {}-byte records",
                fields * 4
            ),
        ))
    }
}
