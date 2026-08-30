//! Typed 64-by-64 terrain tile identities from the WDT `MAIN` grid.

/// Number of ADT tiles along either axis of a stock map.
pub const TERRAIN_MAP_WIDTH: u8 = 64;

/// Validated coordinates of one ADT tile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerrainTileIndex {
    x: u8,
    y: u8,
}

impl TerrainTileIndex {
    /// Creates a tile index inside the stock 64-by-64 map grid.
    #[must_use]
    pub const fn new(x: u8, y: u8) -> Option<Self> {
        if x < TERRAIN_MAP_WIDTH && y < TERRAIN_MAP_WIDTH {
            Some(Self { x, y })
        } else {
            None
        }
    }

    pub(super) fn clamped(x: i32, y: i32) -> Self {
        Self {
            x: x.clamp(0, 63) as u8,
            y: y.clamp(0, 63) as u8,
        }
    }

    /// Returns the map's ADT X coordinate.
    #[must_use]
    pub const fn x(self) -> u8 {
        self.x
    }

    /// Returns the map's ADT Y coordinate.
    #[must_use]
    pub const fn y(self) -> u8 {
        self.y
    }
}

/// One exact WDT `MAIN` entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainTile {
    index: TerrainTileIndex,
    flags: u32,
    area_id: u32,
}

impl TerrainTile {
    pub(super) const fn new(index: TerrainTileIndex, flags: u32, area_id: u32) -> Self {
        Self {
            index,
            flags,
            area_id,
        }
    }

    /// Returns this entry's grid coordinates.
    #[must_use]
    pub const fn index(self) -> TerrainTileIndex {
        self.index
    }

    /// Returns whether the WDT declares a corresponding ADT.
    #[must_use]
    pub const fn exists(self) -> bool {
        self.flags & 1 != 0
    }

    /// Returns the exact unmodified WDT `MAIN` flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the entry's authored area identifier.
    #[must_use]
    pub const fn area_id(self) -> u32 {
        self.area_id
    }
}
