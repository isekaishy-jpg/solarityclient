//! Exact non-ID WorldChunkSounds and WorldStateZoneSounds record layouts.

use super::zone::{AreaSoundReferences, field, load_table};
use crate::archive::AssetError;
use crate::file_stack::AssetStore;
use std::collections::BTreeMap;

/// Five-word key used by 4CCA30's fixed-width WorldChunkSounds lookup.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorldChunkSoundKey {
    /// Active Map.dbc ID.
    pub map_id: u32,
    /// ADT X/Y coordinates after the native six-bit mask.
    pub tile: [u32; 2],
    /// MCNK X/Y coordinates after the native four-bit mask.
    pub chunk: [u32; 2],
}

/// One ordered world-state override. These eight-word rows have no primary ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldStateZoneSound {
    /// Server world-state field and its required raw value.
    pub state: [u32; 2],
    /// AreaTable ID, matched against zone or subzone.
    pub area_id: u32,
    /// WMOAreaTable ID, matched against root or group rows.
    pub world_model_area_id: u32,
    /// Replacement relations at native normal layer 1 and intro layer 6.
    pub sounds: AreaSoundReferences,
}

/// Authored location overrides preserve file order and distinct table namespaces.
pub struct ZoneSoundOverrideCatalog {
    chunks: BTreeMap<WorldChunkSoundKey, AreaSoundReferences>,
    world_states: Vec<WorldStateZoneSound>,
}

impl ZoneSoundOverrideCatalog {
    /// Loads nine-word chunks and eight-word world states. Native loaders are
    /// 657670/8BD0B0 and 657420/8BCF90 respectively; their extra in-memory index
    /// words are not on-disk primary keys.
    ///
    /// # Errors
    /// Returns a mounted-archive or exact-schema failure.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = load_table(store, "DBFilesClient\\WorldChunkSounds.dbc", 9)?;
        let mut chunks = BTreeMap::new();
        for row in 0..table.header().record_count() {
            // 4CD870 overwrites the existing hash record for a duplicate tuple.
            chunks.insert(
                WorldChunkSoundKey {
                    map_id: field(&table, row, 0)?,
                    tile: [field(&table, row, 1)?, field(&table, row, 2)?],
                    chunk: [field(&table, row, 3)?, field(&table, row, 4)?],
                },
                AreaSoundReferences {
                    intro_music_id: field(&table, row, 5)?,
                    zone_music_id: field(&table, row, 6)?,
                    ambience_id: field(&table, row, 7)?,
                    sound_provider_id: field(&table, row, 8)?,
                    underwater_sound_provider_id: 0,
                },
            );
        }
        let table = load_table(store, "DBFilesClient\\WorldStateZoneSounds.dbc", 8)?;
        let mut world_states = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            world_states.push(WorldStateZoneSound {
                state: [field(&table, row, 0)?, field(&table, row, 1)?],
                area_id: field(&table, row, 2)?,
                world_model_area_id: field(&table, row, 3)?,
                sounds: AreaSoundReferences {
                    intro_music_id: field(&table, row, 4)?,
                    zone_music_id: field(&table, row, 5)?,
                    ambience_id: field(&table, row, 6)?,
                    sound_provider_id: field(&table, row, 7)?,
                    underwater_sound_provider_id: 0,
                },
            });
        }
        Ok(Self {
            chunks,
            world_states,
        })
    }

    /// Looks up one exact coordinate tuple without substituting adjacent chunks.
    pub fn chunk(&self, key: WorldChunkSoundKey) -> Option<AreaSoundReferences> {
        self.chunks.get(&key).copied()
    }

    /// Returns authored order, needed for the native first/last-match rules.
    pub fn world_states(&self) -> &[WorldStateZoneSound] {
        &self.world_states
    }
}
