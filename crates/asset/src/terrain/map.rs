//! WDT-backed map manifest and deterministic terrain asset paths.

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath};
use crate::database::MapDefinition;

use super::map_area::{TERRAIN_MAP_WIDTH, TerrainTile, TerrainTileIndex};
use super::map_chunk::{TerrainChunk, TerrainDoodadPlacement, TerrainWorldModelPlacement};
use super::map_chunk_liquid::TerrainLiquidTable;

const TERRAIN_TILE_COUNT: usize = 4_096;
const TILE_SIZE: f32 = 533.333_3;
const CHUNK_SIZE: f32 = TILE_SIZE / 16.0;
const MAP_OFFSET: f32 = 32.0 * TILE_SIZE;

/// A build-12340 WDT manifest selected through normal archive precedence.
pub struct TerrainMap {
    map_id: u32,
    directory: String,
    source: ArchiveDescriptor,
    flags: u32,
    linked_zone_id: u32,
    global_world_model: Option<TerrainWorldModelPlacement>,
    tiles: Vec<TerrainTile>,
}

/// One decoded WotLK ADT with renderable chunks and resolved placements.
pub struct DecodedTerrainTile {
    index: TerrainTileIndex,
    source: ArchiveDescriptor,
    textures: TerrainTextureTable,
    chunks: Vec<TerrainChunk>,
    doodads: Vec<TerrainDoodadPlacement>,
    world_models: Vec<TerrainWorldModelPlacement>,
    liquids: Option<TerrainLiquidTable>,
}

/// Parallel root-level MTEX and optional MTXF payloads.
pub(super) struct TerrainTextureTable {
    pub(super) paths: Vec<AssetPath>,
    pub(super) flags: Option<Vec<u32>>,
}

impl DecodedTerrainTile {
    pub(super) fn new(
        index: TerrainTileIndex,
        source: ArchiveDescriptor,
        textures: TerrainTextureTable,
        chunks: Vec<TerrainChunk>,
        doodads: Vec<TerrainDoodadPlacement>,
        world_models: Vec<TerrainWorldModelPlacement>,
        liquids: Option<TerrainLiquidTable>,
    ) -> Self {
        Self {
            index,
            source,
            textures,
            chunks,
            doodads,
            world_models,
            liquids,
        }
    }

    /// Returns this ADT's coordinates in the parent WDT.
    #[must_use]
    pub const fn index(&self) -> TerrainTileIndex {
        self.index
    }

    /// Returns the archive selected by same-path precedence.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns normalized terrain texture paths indexed by MCLY layers.
    #[must_use]
    pub fn textures(&self) -> &[AssetPath] {
        &self.textures.paths
    }

    /// Returns raw MTXF words parallel to MTEX when the chunk is authored.
    #[must_use]
    pub fn texture_flags(&self) -> Option<&[u32]> {
        self.textures.flags.as_deref()
    }

    /// Returns the complete row-major 16-by-16 MCNK grid.
    #[must_use]
    pub fn chunks(&self) -> &[TerrainChunk] {
        &self.chunks
    }

    /// Returns all resolved MDDF placements in file order.
    #[must_use]
    pub fn doodads(&self) -> &[TerrainDoodadPlacement] {
        &self.doodads
    }

    /// Returns all resolved MODF placements in file order.
    #[must_use]
    pub fn world_models(&self) -> &[TerrainWorldModelPlacement] {
        &self.world_models
    }

    /// Returns whether the tile carries an MH2O liquid table.
    #[must_use]
    pub const fn has_liquid_table(&self) -> bool {
        self.liquids.is_some()
    }

    /// Returns the normalized MH2O table when the root ADT authors one.
    #[must_use]
    pub const fn liquids(&self) -> Option<&TerrainLiquidTable> {
        self.liquids.as_ref()
    }

    /// Resolves the MCNK `AreaTableID` containing one server-space position.
    ///
    /// Stock `0x007A0490` reads this ID from the resident MCNK selected by the
    /// location query. MCNK base positions are their north-east corners;
    /// half-open bounds assign a shared edge to exactly one adjacent chunk.
    #[must_use]
    pub fn area_id_at_world_position(&self, world_x: f32, world_y: f32) -> Option<u32> {
        self.chunks
            .iter()
            .find(|chunk| {
                let [base_x, base_y, _height] = chunk.position();
                world_x <= base_x
                    && world_x > base_x - CHUNK_SIZE
                    && world_y <= base_y
                    && world_y > base_y - CHUNK_SIZE
            })
            .map(TerrainChunk::area_id)
    }
}

impl TerrainMap {
    pub(super) fn new(
        definition: &MapDefinition,
        source: ArchiveDescriptor,
        flags: u32,
        global_world_model: Option<TerrainWorldModelPlacement>,
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
            linked_zone_id: definition.linked_zone_id(),
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

    /// Returns the sole WDT-authored placement for a WMO-only map.
    #[must_use]
    pub const fn global_world_model(&self) -> Option<&TerrainWorldModelPlacement> {
        self.global_world_model.as_ref()
    }

    /// Returns Map.dbc's base area only for the stock global-WMO branch.
    #[must_use]
    pub const fn global_area_id(&self) -> Option<u32> {
        if self.global_world_model.is_some() && self.linked_zone_id != 0 {
            Some(self.linked_zone_id)
        } else {
            None
        }
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
        // ADT filenames use the client terrain grid's horizontal X/Z order,
        // while server movement uses X/Y. The first filename coordinate is
        // therefore derived from server Y and the second from server X.
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
