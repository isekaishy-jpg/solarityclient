//! Native full-resolution baked terrain-shadow point selection (`7A06A0`).

use glam::Vec3;
use solarity_asset::{TerrainChunkIndex, TerrainShadowMap, TerrainTileIndex};

/// Exact resident tile, chunk and authored shadow texel for one entity position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldEntityTerrainShadowPoint {
    tile: TerrainTileIndex,
    chunk: TerrainChunkIndex,
    texel: [usize; 2],
}

impl WorldEntityTerrainShadowPoint {
    /// Converts world XY with native bounds, float stores and ties-to-even.
    /// Positions outside the map or containing nonfinite coordinates have no shadow.
    #[must_use]
    pub fn new(position: Vec3) -> Option<Self> {
        if !position.is_finite() {
            return None;
        }
        // First native axis is world Y (texture column), second is world X.
        let distance = [position.y, position.x].map(|v| f64::from(17_066.666_f32) - f64::from(v));
        if distance
            .iter()
            .any(|&v| !(0.0..f64::from(34_133.332_f32)).contains(&v))
        {
            return None;
        }
        let round = |v: f32| (f64::from(v) - 0.5).round_ties_even() as i32;
        // The first multiply keeps the original extended subtraction. The
        // second reloads its stored float, as do both later texel multiplies.
        let chunks = [
            round((distance[0] * f64::from(0.03_f32)) as f32),
            round((f64::from(distance[1] as f32) * f64::from(0.03_f32)) as f32),
        ];
        let texel = distance
            .map(|v| (round((f64::from(v as f32) * f64::from(1.92_f32)) as f32) & 63) as usize);
        Some(Self {
            tile: TerrainTileIndex::new(
                ((chunks[0] >> 4) & 63) as u8,
                ((chunks[1] >> 4) & 63) as u8,
            )?,
            chunk: TerrainChunkIndex::new((chunks[0] & 15) as u8, (chunks[1] & 15) as u8)?,
            texel,
        })
    }

    /// Returns the native owning ADT.
    #[must_use]
    pub const fn tile(self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns its owning MCNK.
    #[must_use]
    pub const fn chunk(self) -> TerrainChunkIndex {
        self.chunk
    }

    /// Reads the full-resolution MCSH bit before render-texture edge correction.
    #[must_use]
    pub fn is_shadowed(self, map: &TerrainShadowMap) -> bool {
        map.authored_shadow_at(self.texel[0], self.texel[1])
            .unwrap_or(false)
    }
}

#[cfg(test)]
#[path = "../tests/stock_seed/world_entity_terrain_shadow_native.rs"]
mod tests;
