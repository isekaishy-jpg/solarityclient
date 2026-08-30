//! Public terrain atlas identity and immutable allocation diagnostics.

use solarity_asset::TerrainTileIndex;

/// Stable index into one renderer's terrain material-atlas registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerrainMaterialHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable facts for one device-local blend/shadow atlas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainMaterialResourceInfo {
    tile: TerrainTileIndex,
    extent: (u32, u32),
    byte_count: usize,
}

impl TerrainMaterialResourceInfo {
    pub(super) const fn new(tile: TerrainTileIndex, extent: (u32, u32), byte_count: usize) -> Self {
        Self {
            tile,
            extent,
            byte_count,
        }
    }

    /// Returns the owning ADT coordinate.
    #[must_use]
    pub const fn tile(self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the atlas width and height in texels.
    #[must_use]
    pub const fn extent(self) -> (u32, u32) {
        self.extent
    }

    /// Returns the exact tightly packed RGBA8 upload byte count.
    #[must_use]
    pub const fn byte_count(self) -> usize {
        self.byte_count
    }
}
