//! WMO area identities and per-field sound overrides from build 12340.

use crate::archive::AssetError;
use crate::file_stack::AssetStore;

use super::super::localized::localized_string;
use super::zone::{AreaSoundReferences, field, load_table, sort_unique};

/// Exact WMO root, name-set, and group join used by the location service.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorldModelAreaKey {
    /// Root identifier authored in the WMO header.
    pub root_id: u32,
    /// Name-set identifier selected by the placement.
    pub name_set: u32,
    /// Group identifier; -1 denotes a root-wide row.
    pub group_id: i32,
}

/// One WMO area record, including sound overrides and the AreaTable relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldModelAreaDefinition {
    id: u32,
    key: WorldModelAreaKey,
    sounds: AreaSoundReferences,
    flags: u32,
    area_id: u32,
    name: String,
}

impl WorldModelAreaDefinition {
    /// Returns the WMOAreaTable primary key.
    pub const fn id(&self) -> u32 {
        self.id
    }
    /// Returns the root/name-set/group lookup identity.
    pub const fn key(&self) -> WorldModelAreaKey {
        self.key
    }
    /// Returns the independently inherited sound and provider overrides.
    pub const fn sounds(&self) -> AreaSoundReferences {
        self.sounds
    }
    /// Returns the authored area policy flags without inferring indoor state.
    pub const fn flags(&self) -> u32 {
        self.flags
    }
    /// Returns the exact AreaTable relation.
    pub const fn area_id(&self) -> u32 {
        self.area_id
    }
    /// Returns the exact selected-locale WMO area name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// WMO area records indexed by both primary key and spatial join identity.
pub struct WorldModelAreaCatalog {
    entries: Vec<WorldModelAreaDefinition>,
    by_key: Vec<(WorldModelAreaKey, usize)>,
}

impl WorldModelAreaCatalog {
    /// Loads the native 28-word schema, including the 17-word localized name.
    ///
    /// # Errors
    /// Returns an archive, schema, text, or duplicate primary-key error.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = load_table(store, "DBFilesClient\\WMOAreaTable.dbc", 28)?;
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            entries.push(WorldModelAreaDefinition {
                id: field(&table, row, 0)?,
                key: WorldModelAreaKey {
                    root_id: field(&table, row, 1)?,
                    name_set: field(&table, row, 2)?,
                    group_id: field(&table, row, 3)? as i32,
                },
                sounds: AreaSoundReferences::read(&table, row, 4)?,
                flags: field(&table, row, 9)?,
                area_id: field(&table, row, 10)?,
                name: localized_string(&table, row, 11, store.locale())?,
            });
        }
        sort_unique(&table, &mut entries, WorldModelAreaDefinition::id)?;
        let mut by_key: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, row)| (row.key, index))
            .collect();
        by_key.sort_unstable();
        Ok(Self { entries, by_key })
    }

    /// Finds one exact WMOAreaTable primary key.
    pub fn entry(&self, id: u32) -> Option<&WorldModelAreaDefinition> {
        self.entries
            .binary_search_by_key(&id, WorldModelAreaDefinition::id)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Finds one exact root/name-set/group tuple; inheritance belongs to the caller.
    pub fn area(&self, key: WorldModelAreaKey) -> Option<&WorldModelAreaDefinition> {
        let index = self
            .by_key
            .partition_point(|(candidate, _)| *candidate < key);
        self.by_key
            .get(index)
            .filter(|(candidate, _)| *candidate == key)
            .map(|(_, index)| &self.entries[*index])
    }
}
