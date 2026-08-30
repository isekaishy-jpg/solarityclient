//! WDT-backed map manifest and deterministic terrain asset paths.

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath};
use crate::database::MapDefinition;

use super::map_area::{TERRAIN_MAP_WIDTH, TerrainTile, TerrainTileIndex};

const TERRAIN_TILE_COUNT: usize = 4_096;
const TILE_SIZE: f32 = 533.333_3;
const MAP_OFFSET: f32 = 32.0 * TILE_SIZE;

/// A build-12340 WDT manifest selected through normal archive precedence.
pub struct TerrainMap {
    map_id: u32,
    directory: String,
    source: ArchiveDescriptor,
    flags: u32,
    global_world_model: Option<AssetPath>,
    tiles: Vec<TerrainTile>,
}

impl TerrainMap {
    pub(super) fn new(
        definition: &MapDefinition,
        source: ArchiveDescriptor,
        flags: u32,
        global_world_model: Option<AssetPath>,
        tiles: Vec<TerrainTile>,
    ) -> Result<Self, AssetError> {
        if tiles.len() != TERRAIN_TILE_COUNT {
            return Err(AssetError::TerrainDecode {
                path: Self::wdt_path(definition)?,
                message: format!(
                    "WDT MAIN requires {TERRAIN_TILE_COUNT} entries; found {}",
                    tiles.len()
                ),
            });
        }
        Ok(Self {
            map_id: definition.id(),
            directory: definition.directory().to_owned(),
            source,
            flags,
            global_world_model,
            tiles,
        })
    }

    /// Returns the numeric `Map.dbc` identity.
    #[must_use]
    pub const fn map_id(&self) -> u32 {
        self.map_id
    }

    /// Returns the exact client-authored map directory stem.
    #[must_use]
    pub fn directory(&self) -> &str {
        &self.directory
    }

    /// Returns the archive selected for the WDT.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns the unmodified WDT `MPHD` flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the global WMO path for a WMO-only map.
    #[must_use]
    pub const fn global_world_model(&self) -> Option<&AssetPath> {
        self.global_world_model.as_ref()
    }

    /// Returns one exact WDT tile entry.
    #[must_use]
    pub fn tile(&self, index: TerrainTileIndex) -> &TerrainTile {
        &self.tiles
            [usize::from(index.y()) * usize::from(TERRAIN_MAP_WIDTH) + usize::from(index.x())]
    }

    /// Iterates only ADT-backed entries in deterministic row-major order.
    pub fn existing_tiles(&self) -> impl Iterator<Item = &TerrainTile> {
        self.tiles.iter().filter(|tile| tile.exists())
    }

    /// Maps stock world X/Y coordinates to the clamped ADT grid.
    #[must_use]
    pub fn tile_at_world_position(world_x: f32, world_y: f32) -> TerrainTileIndex {
        let tile_x = ((MAP_OFFSET - world_y) / TILE_SIZE).floor() as i32;
        let tile_y = ((MAP_OFFSET - world_x) / TILE_SIZE).floor() as i32;
        // The server can briefly report coordinates beyond a map boundary
        // during transfers. Stock clamps these only for tile addressing.
        TerrainTileIndex::clamped(tile_x, tile_y)
    }

    /// Builds the exact WDT path from the client-authored directory.
    pub fn wdt_path(definition: &MapDefinition) -> Result<AssetPath, AssetError> {
        AssetPath::new(format!("World\\Maps\\{0}\\{0}.wdt", definition.directory()))
    }

    /// Builds the exact ADT path for one existing map tile.
    pub fn adt_path(&self, index: TerrainTileIndex) -> Result<AssetPath, AssetError> {
        AssetPath::new(format!(
            "World\\Maps\\{0}\\{0}_{1}_{2}.adt",
            self.directory,
            index.x(),
            index.y()
        ))
    }

    /// Builds the exact low-detail terrain path for this map.
    pub fn wdl_path(&self) -> Result<AssetPath, AssetError> {
        AssetPath::new(format!("World\\Maps\\{0}\\{0}.wdl", self.directory))
    }
}
