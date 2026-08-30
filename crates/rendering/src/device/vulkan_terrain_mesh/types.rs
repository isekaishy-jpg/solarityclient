//! Public terrain geometry identity and immutable allocation diagnostics.

use solarity_asset::TerrainTileIndex;

/// Stable index into one renderer's terrain geometry registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerrainMeshHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable payload facts retained beside one device-local ADT allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainMeshResourceInfo {
    tile: TerrainTileIndex,
    vertex_count: usize,
    index_count: usize,
    chunk_count: usize,
    vertex_byte_count: usize,
    index_byte_count: usize,
}

impl TerrainMeshResourceInfo {
    pub(super) const fn new(
        tile: TerrainTileIndex,
        vertex_count: usize,
        index_count: usize,
        chunk_count: usize,
        vertex_byte_count: usize,
        index_byte_count: usize,
    ) -> Self {
        Self {
            tile,
            vertex_count,
            index_count,
            chunk_count,
            vertex_byte_count,
            index_byte_count,
        }
    }

    /// Returns the owning ADT coordinate.
    #[must_use]
    pub const fn tile(self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the number of vertices in the shared ADT buffer.
    #[must_use]
    pub const fn vertex_count(self) -> usize {
        self.vertex_count
    }

    /// Returns the number of compact direct indices available to draw.
    #[must_use]
    pub const fn index_count(self) -> usize {
        self.index_count
    }

    /// Returns the number of retained MCNK draw ranges.
    #[must_use]
    pub const fn chunk_count(self) -> usize {
        self.chunk_count
    }

    /// Returns the unpadded serialized vertex payload size.
    #[must_use]
    pub const fn vertex_byte_count(self) -> usize {
        self.vertex_byte_count
    }

    /// Returns the unpadded serialized index payload size.
    #[must_use]
    pub const fn index_byte_count(self) -> usize {
        self.index_byte_count
    }
}
